//! `RulesLens`: the schema lens with the effective rules on top. Per column the strongest
//! reference wins: declared foreign key, then project, then preset, then same-name inference.
//! Kind rules apply where no key or reference does. Conditions, meta-key kinds and value styles
//! are decided per row (`cells`), in memory over the page.

use std::collections::HashMap;
use std::sync::Arc;

use super::condition::{self, Condition};
use super::effective::{
    EffectiveRules, GroupKey, MetaRef, RuleReference, RuleStyle, inferred_id, meta_keys_match,
};
use super::kinds::ValueRule;
use crate::db::{Cell, ColumnMeta, ResultSet};
use crate::lens::{ColumnsRef, Lens, ResolvedReference, SchemaLens, TableColumns};
use crate::value::{
    CellAnnotations, CellKind, CellStyle, ColumnKind, ReferenceSource, ReferenceTarget,
};

pub struct RulesLens {
    schema: SchemaLens,
    rules: Arc<EffectiveRules>,
}

/// Rule references of one source, column and condition: one reference with all their targets.
struct Group<'r> {
    key: &'r GroupKey,
    by: &'r ReferenceSource,
    when: &'r [Condition],
    rules: Vec<&'r RuleReference>,
}

impl RulesLens {
    pub fn new(schema: SchemaLens, rules: Arc<EffectiveRules>) -> Self {
        Self { schema, rules }
    }

    /// The rule references that decide the kind of `table.column`, grouped, in precedence order.
    /// Empty for a column with a declared foreign key (a constraint is a fact no rule overrides)
    /// and for the table's single-column key (the row's identity).
    fn groups(&self, table: &str, column: &str) -> Vec<Group<'_>> {
        let own_key = self
            .schema
            .single_key(table)
            .is_some_and(|k| k.eq_ignore_ascii_case(column));
        if own_key || self.schema.has_declared_foreign_key(table, column) {
            return Vec::new();
        }
        let mut groups: Vec<Group<'_>> = Vec::new();
        let from_here = self
            .rules
            .references
            .iter()
            .filter(|r| r.from.table == table && r.from.column.eq_ignore_ascii_case(column));
        for rule in from_here {
            match groups.iter_mut().find(|g| *g.key == rule.group) {
                Some(group) => group.rules.push(rule),
                None => groups.push(Group {
                    key: &rule.group,
                    by: &rule.by,
                    when: &rule.when,
                    rules: vec![rule],
                }),
            }
        }
        groups
    }

    /// The group that holds for every row, if any: it decides the column's kind.
    fn unconditional_group(&self, table: &str, column: &str) -> Option<Group<'_>> {
        self.groups(table, column)
            .into_iter()
            .find(|g| g.when.is_empty())
    }

    /// The precedence of a column or a cell: a reference rule wins; otherwise a kind rule
    /// refines a kind that only the SQL type gave. `None` keeps `base`.
    fn decide(
        &self,
        group: Option<&Group<'_>>,
        base: &ColumnKind,
        kind_rule: impl FnOnce() -> Option<ColumnKind>,
    ) -> Option<ColumnKind> {
        if let Some(group) = group {
            return Some(self.reference_kind(group));
        }
        if !is_type_kind(base) {
            return None;
        }
        kind_rule()
    }

    fn reference_kind(&self, group: &Group<'_>) -> ColumnKind {
        let targets = group.rules.iter().map(|r| self.target(r)).collect();
        ColumnKind::Reference {
            targets,
            by: group.by.clone(),
        }
    }

    fn target(&self, rule: &RuleReference) -> ReferenceTarget {
        let to = &rule.to;
        let key = rule.to_filter.is_empty()
            && self
                .schema
                .single_key(&to.table)
                .is_some_and(|k| k.eq_ignore_ascii_case(&to.column));
        ReferenceTarget {
            table: self
                .schema
                .filtered_address(&to.table, rule.to_filter.clone()),
            column: to.column.clone(),
            key,
        }
    }

    fn infers(&self, table: &str, column: &str) -> bool {
        !self.rules.disabled.contains(&inferred_id(table, column))
    }

    fn column_kind(&self, table: &str, column: &ColumnMeta) -> ColumnKind {
        let base = self
            .schema
            .column_kind(table, column, self.infers(table, &column.name));
        let group = self.unconditional_group(table, &column.name);
        let kind_rule = || {
            self.rules
                .column_kind_rule(table, &column.name)
                .map(|rule| rule_kind(&rule.value))
        };
        self.decide(group.as_ref(), &base, kind_rule)
            .unwrap_or(base)
    }

    fn meta_kind(&self, meta: &MetaRef, key: &str) -> Option<ColumnKind> {
        self.rules
            .meta_kind_rule(meta, key)
            .map(|rule| rule_kind(&rule.value))
    }

    fn rule_reference(&self, rule: &RuleReference) -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new(&rule.from.table, vec![rule.from.column.clone()]),
            to: ColumnsRef::new(&rule.to.table, vec![rule.to.column.clone()]),
            by: rule.by.clone(),
        }
    }

    /// Schema references minus inferred ones that are disabled or that an unconditional rule
    /// replaces.
    fn keep_schema_reference(&self, reference: &ResolvedReference) -> bool {
        if !matches!(reference.by, ReferenceSource::SameName { .. }) {
            return true;
        }
        let [column] = reference.from.columns.as_slice() else {
            return true;
        };
        let table = &reference.from.table;
        self.infers(table, column) && self.unconditional_group(table, column).is_none()
    }

    fn rule_references(&self, keep: impl Fn(&RuleReference) -> bool) -> Vec<ResolvedReference> {
        self.rules
            .references
            .iter()
            .filter(|r| keep(r))
            .filter(|r| {
                !self
                    .schema
                    .has_declared_foreign_key(&r.from.table, &r.from.column)
            })
            .map(|r| self.rule_reference(r))
            .collect()
    }
}

impl Lens for RulesLens {
    fn kinds(&self, table: &str, columns: &[ColumnMeta]) -> Vec<ColumnKind> {
        columns.iter().map(|c| self.column_kind(table, c)).collect()
    }

    fn references_from(&self, table: &str) -> Vec<ResolvedReference> {
        let mut references = self.rule_references(|r| r.from.table == table);
        let schema = self.schema.references_from(table);
        references.extend(schema.into_iter().filter(|r| self.keep_schema_reference(r)));
        references
    }

    fn references_to(&self, table: &str) -> Vec<ResolvedReference> {
        let mut references = self.rule_references(|r| r.to.table == table);
        let schema = self.schema.references_to(table);
        references.extend(schema.into_iter().filter(|r| self.keep_schema_reference(r)));
        references
    }

    fn cells(&self, columns: &TableColumns<'_>, result: &ResultSet) -> CellAnnotations {
        let mut out = CellAnnotations::default();
        let table = columns.table;
        let by_name: HashMap<String, usize> = columns
            .columns
            .iter()
            .map(|(index, name)| (name.to_ascii_lowercase(), *index))
            .collect();
        let meta = self.rules.meta_table(table);
        for (col, name) in &columns.columns {
            let column = ColumnCells::new(self, table, name, meta, &by_name);
            if column.is_idle() {
                continue;
            }
            let column_kind = result.kinds.get(*col).unwrap_or(&ColumnKind::Text);
            for row in 0..result.rows.len() {
                let value = |c: &str| {
                    by_name
                        .get(&c.to_ascii_lowercase())
                        .map(|i| text(result, row, *i))
                };
                if let Some(kind) = column.kind(column_kind, &value)
                    && kind != *column_kind
                {
                    out.kinds.push(CellKind {
                        row,
                        col: *col,
                        kind,
                    });
                }
                if let Some(style) = column.style(text(result, row, *col), &value) {
                    out.styles.push(CellStyle {
                        row,
                        col: *col,
                        style: style.to_string(),
                    });
                }
            }
        }
        out
    }
}

/// A row's text by column name: `None` when the result does not have the column (a condition on
/// it cannot hold), `Some(None)` for NULL or a cell without text.
type RowValue<'a> = dyn Fn(&str) -> Option<Option<&'a str>> + 'a;

/// The rules one column's cells depend on, gathered once per page.
struct ColumnCells<'l> {
    lens: &'l RulesLens,
    /// Conditional groups that come before the column's unconditional one (later ones never win).
    conditional: Vec<Group<'l>>,
    /// The meta table whose value column this is, when its key column is in the result.
    meta: Option<&'l MetaRef>,
    /// Styles scoped to this column, and meta-key styles when it is a meta value column.
    styles: Vec<(&'l RuleStyle, bool)>,
}

impl<'l> ColumnCells<'l> {
    fn new(
        lens: &'l RulesLens,
        table: &str,
        column: &str,
        meta: Option<&'l MetaRef>,
        by_name: &HashMap<String, usize>,
    ) -> Self {
        let conditional = lens
            .groups(table, column)
            .into_iter()
            .take_while(|g| !g.when.is_empty())
            .collect();
        // A meta value column without its key column in the result cannot be decided per row.
        let meta = meta.filter(|m| {
            m.value.eq_ignore_ascii_case(column)
                && by_name.contains_key(&m.key.to_ascii_lowercase())
        });
        let styles = lens
            .rules
            .styles
            .iter()
            .filter_map(|s| {
                let by_column = lens.rules.columns_match(&s.rule.columns, table, column);
                let by_key = meta.is_some() && !s.rule.meta_keys.is_empty();
                (by_column || by_key).then_some((s, by_column))
            })
            .collect();
        Self {
            lens,
            conditional,
            meta,
            styles,
        }
    }

    fn is_idle(&self) -> bool {
        self.conditional.is_empty() && self.meta.is_none() && self.styles.is_empty()
    }

    fn key<'a>(&self, value: &RowValue<'a>) -> Option<&'a str> {
        value(&self.meta?.key).flatten()
    }

    /// A conditional reference that holds on the row, else the kind rule of the row's meta key.
    fn kind(&self, column_kind: &ColumnKind, value: &RowValue<'_>) -> Option<ColumnKind> {
        let group = self
            .conditional
            .iter()
            .find(|g| condition::holds(g.when, value));
        let meta_rule = || self.lens.meta_kind(self.meta?, self.key(value)?);
        self.lens.decide(group, column_kind, meta_rule)
    }

    fn style(&self, cell: Option<&str>, value: &RowValue<'_>) -> Option<&'static str> {
        let key = self.key(value);
        self.styles
            .iter()
            .find(|(style, by_column)| {
                let scoped = *by_column
                    || match (self.meta, key) {
                        (Some(meta), Some(key)) => {
                            meta_keys_match(&style.rule.meta_keys, &style.rule.r#in, meta, key)
                        }
                        _ => false,
                    };
                scoped && style.matcher.matches(cell)
            })
            .map(|(style, _)| style.rule.color.name())
    }
}

/// Kinds that only the SQL type gave, which kind rules may refine.
fn is_type_kind(kind: &ColumnKind) -> bool {
    matches!(
        kind,
        ColumnKind::Text | ColumnKind::Number | ColumnKind::Timestamp { .. }
    )
}

fn rule_kind(rule: &ValueRule) -> ColumnKind {
    match rule {
        ValueRule::Money(money) => ColumnKind::Money {
            currency: money.currency.clone(),
        },
        ValueRule::Timestamp(zone) => ColumnKind::Timestamp { zone: Some(*zone) },
        ValueRule::Enum => ColumnKind::Enum,
        ValueRule::Secret => ColumnKind::Secret,
        ValueRule::Number => ColumnKind::Number,
    }
}

/// Conditions and styles must see the whole value, not the preview of a long cell.
fn text(result: &ResultSet, row: usize, col: usize) -> Option<&str> {
    match result.rows.get(row)?.get(col)? {
        Cell::LongText { .. } => result
            .full_texts
            .iter()
            .find(|f| f.row == row && f.col == col)
            .map(|f| f.text.as_str()),
        cell => cell.as_text(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::{Address, Target};
    use crate::db::{ColumnEquals, ForeignKey};
    use crate::rules::kinds::{KindRule, StyleColor, StyleMatch, StyleRule, Zone};
    use crate::rules::project::Project;
    use crate::rules::testing::{
        forum, forum_schema, preset, project, reference, schema, with_foreign_key,
    };
    use crate::rules::types::{ColumnGlob, Preset};
    use crate::schema::DatabaseSchema;
    use crate::value::{ValueKind, value_at};

    fn lens_over(schema: DatabaseSchema, project: &Project, presets: &[Arc<Preset>]) -> RulesLens {
        let schema = Arc::new(schema);
        let rules = Arc::new(EffectiveRules::build(project, &schema, presets));
        RulesLens::new(SchemaLens::new(schema, "local", "wp"), rules)
    }

    fn lens(project: &Project) -> RulesLens {
        lens_over(forum_schema(), project, &[forum()])
    }

    fn col(name: &str, type_name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            type_name: type_name.into(),
            origin: None,
        }
    }

    fn kind_of(lens: &RulesLens, table: &str, name: &str, type_name: &str) -> ColumnKind {
        lens.kinds(table, &[col(name, type_name)]).remove(0)
    }

    fn address(table: &str) -> Address {
        filtered(table, &[])
    }

    fn filtered(table: &str, filter: &[(&str, &str)]) -> Address {
        let filter = filter
            .iter()
            .map(|(column, value)| ColumnEquals {
                column: column.to_string(),
                value: Some(value.to_string()),
            })
            .collect();
        let target = Target::Table {
            table: table.into(),
            filter,
        };
        Address::new("local", "wp", target)
    }

    fn row_address(table: &str, key: &str) -> Address {
        let target = Target::Row {
            table: table.into(),
            key: key.into(),
        };
        Address::new("local", "wp", target)
    }

    fn target(table: &str, column: &str, key: bool) -> ReferenceTarget {
        ReferenceTarget {
            table: address(table),
            column: column.into(),
            key,
        }
    }

    fn by_preset(preset: &str, id: &str) -> ReferenceSource {
        ReferenceSource::Preset {
            preset: preset.into(),
            id: id.into(),
        }
    }

    fn by_forum(id: &str) -> ReferenceSource {
        by_preset("forum", id)
    }

    fn by_project(id: &str) -> ReferenceSource {
        ReferenceSource::Project { id: id.into() }
    }

    fn refkind(targets: Vec<ReferenceTarget>, by: ReferenceSource) -> ColumnKind {
        ColumnKind::Reference { targets, by }
    }

    fn resolved(from: (&str, &str), to: (&str, &str), by: ReferenceSource) -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new(from.0, vec![from.1.into()]),
            to: ColumnsRef::new(to.0, vec![to.1.into()]),
            by,
        }
    }

    fn from_column(references: Vec<ResolvedReference>, column: &str) -> Vec<ResolvedReference> {
        references
            .into_iter()
            .filter(|r| r.from.columns == [column])
            .collect()
    }

    fn glob(text: &str) -> ColumnGlob {
        ColumnGlob::try_from(text.to_string()).unwrap()
    }

    fn text_cell(value: Option<&str>) -> Cell {
        value.map_or(Cell::Null, |v| Cell::Text(v.into()))
    }

    /// The result of reading `rows` of `table`, with the lens's kinds filled in, and what the
    /// lens decides per cell.
    fn page(
        lens: &RulesLens,
        table: &str,
        columns: &[(&str, &str)],
        rows: Vec<Vec<Cell>>,
    ) -> (ResultSet, CellAnnotations) {
        let mut result = ResultSet {
            columns: columns.iter().map(|(n, t)| col(n, t)).collect(),
            rows,
            ..ResultSet::default()
        };
        result.kinds = lens.kinds(table, &result.columns);
        let names = columns
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (i, *name))
            .collect();
        let cells = lens.cells(
            &TableColumns {
                table,
                columns: names,
            },
            &result,
        );
        (result, cells)
    }

    const NOTES: [(&str, &str); 4] = [
        ("note_id", "BIGINT"),
        ("thread_id", "BIGINT"),
        ("k", "VARCHAR"),
        ("v", "TEXT"),
    ];

    fn note(k: Option<&str>, v: Option<&str>) -> Vec<Cell> {
        vec![
            Cell::Number("1".into()),
            Cell::Number("1".into()),
            text_cell(k),
            text_cell(v),
        ]
    }

    const SETTINGS: [(&str, &str); 3] = [
        ("setting_id", "BIGINT"),
        ("name", "VARCHAR"),
        ("val", "TEXT"),
    ];

    fn setting(name: Option<&str>, val: &str) -> Vec<Cell> {
        vec![
            Cell::Number("1".into()),
            text_cell(name),
            text_cell(Some(val)),
        ]
    }

    fn cell_kind(row: usize, col: usize, kind: ColumnKind) -> CellKind {
        CellKind { row, col, kind }
    }

    fn cell_style(row: usize, col: usize, style: &str) -> CellStyle {
        CellStyle {
            row,
            col,
            style: style.into(),
        }
    }

    #[test]
    fn a_preset_reference_makes_its_column_a_reference() {
        let lens = lens(&project());

        assert_eq!(
            kind_of(&lens, "f_threads", "author", "BIGINT"),
            refkind(
                vec![target("f_people", "person_id", true)],
                by_forum("threads.author")
            )
        );
    }

    #[test]
    fn same_name_inference_still_applies_where_no_rule_names_the_column() {
        let lens = lens(&project());

        assert_eq!(
            kind_of(&lens, "f_threads", "person_id", "BIGINT"),
            refkind(
                vec![target("f_people", "person_id", true)],
                ReferenceSource::SameName {
                    column: "person_id".into(),
                    indexed: false
                }
            )
        );
    }

    #[test]
    fn the_fk_of_an_owned_meta_table_is_a_preset_reference_that_replaces_the_inference() {
        let lens = lens(&project());
        let implied = by_forum("notes.thread_id");

        assert_eq!(
            kind_of(&lens, "f_notes", "thread_id", "BIGINT"),
            refkind(
                vec![target("f_threads", "thread_id", true)],
                implied.clone()
            )
        );
        assert_eq!(
            from_column(lens.references_from("f_notes"), "thread_id"),
            [resolved(
                ("f_notes", "thread_id"),
                ("f_threads", "thread_id"),
                implied
            )]
        );
    }

    #[test]
    fn disabling_the_implied_meta_reference_leaves_the_same_name_inference() {
        let own = Project {
            disabled: vec!["notes.thread_id".into()],
            ..project()
        };

        assert_eq!(
            kind_of(&lens(&own), "f_notes", "thread_id", "BIGINT"),
            refkind(
                vec![target("f_threads", "thread_id", true)],
                ReferenceSource::SameName {
                    column: "thread_id".into(),
                    indexed: false
                }
            )
        );
    }

    #[test]
    fn a_project_reference_replaces_the_preset_rule_with_the_same_from() {
        let own = Project {
            references: vec![reference("mine", "threads.author", "threads.thread_id")],
            ..project()
        };
        let lens = lens(&own);

        assert_eq!(
            kind_of(&lens, "f_threads", "author", "BIGINT"),
            refkind(
                vec![target("f_threads", "thread_id", true)],
                by_project("mine")
            )
        );
        assert_eq!(
            from_column(lens.references_from("f_threads"), "author"),
            [resolved(
                ("f_threads", "author"),
                ("f_threads", "thread_id"),
                by_project("mine")
            )]
        );
    }

    #[test]
    fn a_declared_foreign_key_wins_over_project_and_preset_rules() {
        let fk = ForeignKey {
            name: "threads_author".into(),
            columns: vec!["author".into()],
            ref_table: "f_people".into(),
            ref_columns: vec!["person_id".into()],
        };
        let own = Project {
            references: vec![reference("mine", "threads.author", "threads.thread_id")],
            ..project()
        };
        let lens = lens_over(
            with_foreign_key(forum_schema(), "f_threads", fk),
            &own,
            &[forum()],
        );
        let declared = ReferenceSource::ForeignKey {
            constraint: "threads_author".into(),
        };

        assert_eq!(
            kind_of(&lens, "f_threads", "author", "BIGINT"),
            refkind(
                vec![target("f_people", "person_id", true)],
                declared.clone()
            )
        );
        assert_eq!(
            from_column(lens.references_from("f_threads"), "author"),
            [resolved(
                ("f_threads", "author"),
                ("f_people", "person_id"),
                declared
            )]
        );
    }

    #[test]
    fn a_rule_does_not_turn_the_tables_own_key_into_a_reference() {
        let own = Project {
            references: vec![reference("mine", "threads.thread_id", "people.person_id")],
            ..project()
        };

        assert_eq!(
            kind_of(&lens(&own), "f_threads", "thread_id", "BIGINT"),
            ColumnKind::PrimaryKey {
                table: address("f_threads"),
                columns: vec!["thread_id".into()],
            }
        );
    }

    #[test]
    fn a_disabled_preset_reference_is_dropped() {
        let own = Project {
            disabled: vec!["threads.author".into()],
            ..project()
        };
        let lens = lens(&own);

        assert_eq!(
            kind_of(&lens, "f_threads", "author", "BIGINT"),
            ColumnKind::Number
        );
        assert!(from_column(lens.references_from("f_threads"), "author").is_empty());
    }

    #[test]
    fn a_disabled_inferred_reference_is_dropped_from_kinds_and_references() {
        let enabled = lens(&project());
        let own = Project {
            disabled: vec!["inferred:f_threads.person_id".into()],
            ..project()
        };
        let disabled = lens(&own);
        let inferred_from_threads = |lens: &RulesLens| {
            lens.references_to("f_people")
                .into_iter()
                .filter(|r| {
                    r.from.table == "f_threads" && matches!(r.by, ReferenceSource::SameName { .. })
                })
                .count()
        };

        assert_eq!(
            from_column(enabled.references_from("f_threads"), "person_id").len(),
            1
        );
        assert_eq!(inferred_from_threads(&enabled), 1);
        assert_eq!(
            kind_of(&disabled, "f_threads", "person_id", "BIGINT"),
            ColumnKind::Number
        );
        assert!(from_column(disabled.references_from("f_threads"), "person_id").is_empty());
        assert_eq!(inferred_from_threads(&disabled), 0);
    }

    #[test]
    fn an_unconditional_rule_replaces_the_inference_on_its_column() {
        let own = Project {
            references: vec![reference("mine", "threads.person_id", "people.name")],
            ..project()
        };
        let lens = lens(&own);

        assert_eq!(
            kind_of(&lens, "f_threads", "person_id", "BIGINT"),
            refkind(vec![target("f_people", "name", false)], by_project("mine"))
        );
        assert_eq!(
            from_column(lens.references_from("f_threads"), "person_id"),
            [resolved(
                ("f_threads", "person_id"),
                ("f_people", "name"),
                by_project("mine")
            )]
        );
    }

    #[test]
    fn rules_of_one_source_on_one_column_make_one_reference_with_both_targets() {
        let lens = lens(&project());
        let first = by_forum("replies.by->people.person_id");
        let second = by_forum("replies.by->bots.bot_id");

        assert_eq!(
            kind_of(&lens, "f_replies", "by", "BIGINT"),
            refkind(
                vec![
                    target("f_people", "person_id", true),
                    target("f_bots", "bot_id", true)
                ],
                first.clone()
            )
        );
        assert_eq!(
            from_column(lens.references_from("f_replies"), "by"),
            [
                resolved(("f_replies", "by"), ("f_people", "person_id"), first),
                resolved(("f_replies", "by"), ("f_bots", "bot_id"), second.clone()),
            ]
        );
        assert_eq!(
            lens.references_to("f_bots"),
            [resolved(("f_replies", "by"), ("f_bots", "bot_id"), second)]
        );
    }

    #[test]
    fn network_table_references_point_at_the_network_prefix() {
        let net = preset(
            "[preset]\nname = \"net\"\nmarker_tables = [\"posts\"]\nnetwork_tables = [\"users\"]\n\n[[reference]]\nfrom = \"posts.author\"\nto = \"users.id\"\n",
        );
        let own = Project {
            prefix: Some("x2_".into()),
            network_prefix: Some("x_".into()),
            ..project()
        };
        let lens = lens_over(
            schema(&[
                ("x_users", &["id", "name"], &["id"]),
                ("x2_posts", &["post_id", "author"], &["post_id"]),
            ]),
            &own,
            &[net],
        );
        let expected = resolved(
            ("x2_posts", "author"),
            ("x_users", "id"),
            by_preset("net", "posts.author"),
        );

        assert_eq!(
            kind_of(&lens, "x2_posts", "author", "BIGINT"),
            refkind(
                vec![target("x_users", "id", true)],
                by_preset("net", "posts.author")
            )
        );
        assert_eq!(
            lens.references_from("x2_posts"),
            std::slice::from_ref(&expected)
        );
        assert_eq!(
            lens.references_to("x_users"),
            std::slice::from_ref(&expected)
        );
    }

    #[test]
    fn column_kinds_come_from_the_rules_that_name_them() {
        let lens = lens(&project());
        let cases = [
            (
                "f_threads",
                "opened_gmt",
                "DATETIME",
                ColumnKind::Timestamp {
                    zone: Some(Zone::Utc),
                },
            ),
            (
                "f_people",
                "joined_gmt",
                "DATETIME",
                ColumnKind::Timestamp {
                    zone: Some(Zone::Utc),
                },
            ),
            (
                "f_threads",
                "closed_local",
                "DATETIME",
                ColumnKind::Timestamp {
                    zone: Some(Zone::Local),
                },
            ),
            ("f_threads", "status", "VARCHAR", ColumnKind::Enum),
            ("f_threads", "title", "VARCHAR", ColumnKind::Text),
            ("f_people", "secret_code", "VARCHAR", ColumnKind::Secret),
            ("f_people", "name", "VARCHAR", ColumnKind::Text),
            (
                "f_invoices",
                "amount",
                "DECIMAL",
                ColumnKind::Money {
                    currency: vec!["currency".into()],
                },
            ),
        ];
        for (table, name, type_name, expected) in cases {
            assert_eq!(
                kind_of(&lens, table, name, type_name),
                expected,
                "{table}.{name}"
            );
        }
    }

    #[test]
    fn a_project_kind_rule_wins_over_a_preset_kind_rule() {
        let own = Project {
            kinds: vec![KindRule {
                columns: vec![glob("threads.status")],
                meta_keys: Vec::new(),
                in_meta: Vec::new(),
                value: crate::rules::kinds::ValueRule::Number,
            }],
            ..project()
        };

        assert_eq!(
            kind_of(&lens(&own), "f_threads", "status", "VARCHAR"),
            ColumnKind::Number
        );
    }

    #[test]
    fn a_conditional_reference_decides_per_row_by_its_key_and_leaves_other_rows_alone() {
        let lens = lens(&project());
        let rows = vec![
            note(Some("_ref"), Some("7")),
            note(Some("_whoami"), Some("3")),
            note(Some("other"), Some("x")),
            note(None, Some("7")),
        ];

        let (result, cells) = page(&lens, "f_notes", &NOTES, rows);

        assert_eq!(result.kinds[3], ColumnKind::Text);
        assert_eq!(
            cells.kinds,
            [
                cell_kind(
                    0,
                    3,
                    refkind(
                        vec![target("f_threads", "thread_id", true)],
                        by_forum("notes.v?k=_ref")
                    )
                ),
                cell_kind(
                    1,
                    3,
                    refkind(
                        vec![target("f_people", "person_id", true)],
                        by_forum("notes.v?k=glob(_who*)")
                    )
                ),
            ]
        );
        assert!(cells.styles.is_empty());
    }

    #[test]
    fn a_conditional_reference_to_a_non_key_column_filters_the_target_by_its_to_when() {
        let lens = lens(&project());
        let rows = vec![
            note(Some("_ref"), Some("7")),
            note(Some("_ext"), Some("abc")),
            note(Some("other"), Some("x")),
        ];
        let (mut result, cells) = page(&lens, "f_notes", &NOTES, rows);
        result.cell_kinds = cells.kinds;

        let kind = |row| value_at(&result, row, 3, None).kind;

        assert_eq!(
            kind(0),
            ValueKind::Reference {
                targets: vec![row_address("f_threads", "7")],
                by: by_forum("notes.v?k=_ref"),
                exists: None,
            }
        );
        assert_eq!(
            kind(1),
            ValueKind::Reference {
                targets: vec![filtered("f_notes", &[("k", "_ext"), ("v", "abc")])],
                by: by_forum("notes.v?k=_ext"),
                exists: None,
            }
        );
        assert_eq!(kind(2), ValueKind::Text);
    }

    #[test]
    fn a_conditional_reference_needs_its_condition_column_in_the_result() {
        let lens = lens(&project());
        let (_, cells) = page(
            &lens,
            "f_notes",
            &[("v", "TEXT")],
            vec![vec![Cell::Text("7".into())]],
        );

        assert_eq!(cells, CellAnnotations::default());
    }

    #[test]
    fn a_regexp_condition_matches_text_and_never_null() {
        let lens = lens(&project());
        let rows = ["42", "guest-1"]
            .iter()
            .map(|key| vec![Cell::Number("1".into()), Cell::Text((*key).into())])
            .chain([vec![Cell::Number("1".into()), Cell::Null]])
            .collect();

        let (_, cells) = page(
            &lens,
            "f_sessions",
            &[("session_id", "BIGINT"), ("skey", "VARCHAR")],
            rows,
        );

        assert_eq!(
            cells.kinds,
            [cell_kind(
                0,
                1,
                refkind(
                    vec![target("f_people", "person_id", true)],
                    by_forum("sessions.skey?skey=regexp(^[0-9]+$)")
                )
            )]
        );
    }

    #[test]
    fn meta_key_kinds_apply_to_a_standalone_key_value_table() {
        let lens = lens(&project());
        let rows = vec![
            setting(Some("api_key"), "abc"),
            setting(Some("expires_at"), "1700000000"),
            setting(Some("tone"), "warm"),
            setting(Some("tone"), "cold"),
            setting(Some("mood"), "warm"),
            setting(None, "x"),
        ];

        let (_, cells) = page(&lens, "f_settings", &SETTINGS, rows);

        assert_eq!(
            cells.kinds,
            [
                cell_kind(0, 2, ColumnKind::Secret),
                cell_kind(
                    1,
                    2,
                    ColumnKind::Timestamp {
                        zone: Some(Zone::Unix)
                    }
                ),
                cell_kind(2, 2, ColumnKind::Enum),
                cell_kind(3, 2, ColumnKind::Enum),
            ]
        );
        assert_eq!(cells.styles, [cell_style(2, 2, "green")]);
    }

    #[test]
    fn meta_key_rules_that_name_other_meta_tables_leave_an_owned_meta_table_alone() {
        let lens = lens(&project());
        let rows = vec![
            note(Some("api_key"), Some("s")),
            note(Some("expires_at"), Some("1700000000")),
            note(Some("tone"), Some("warm")),
        ];

        let (_, cells) = page(&lens, "f_notes", &NOTES, rows);

        assert_eq!(cells.kinds, [cell_kind(0, 3, ColumnKind::Secret)]);
        assert!(cells.styles.is_empty());
    }

    #[test]
    fn a_value_style_colors_matching_values_of_its_own_columns_only() {
        let lens = lens(&project());
        let rows = vec![
            vec![
                num("1"),
                text_cell(Some("closed")),
                text_cell(Some("closed")),
            ],
            vec![num("2"), text_cell(Some("x")), text_cell(Some("open"))],
            vec![
                num("3"),
                text_cell(Some("draft")),
                text_cell(Some("closed")),
            ],
            vec![num("4"), text_cell(Some("draft")), text_cell(None)],
        ];

        let (_, cells) = page(&lens, "f_threads", &THREADS, rows);

        assert!(cells.kinds.is_empty());
        assert_eq!(
            cells.styles,
            [cell_style(0, 2, "red"), cell_style(2, 2, "red")]
        );
    }

    #[test]
    fn a_project_switches_a_preset_style_on_by_adding_it_enabled() {
        let own = Project {
            styles: vec![StyleRule {
                columns: vec![glob("threads.title")],
                meta_keys: Vec::new(),
                r#in: Vec::new(),
                matching: StyleMatch::Eq,
                value: "draft".into(),
                color: StyleColor::Blue,
                enabled: true,
            }],
            ..project()
        };
        let lens = lens(&own);
        let rows = vec![vec![
            num("1"),
            text_cell(Some("draft")),
            text_cell(Some("open")),
        ]];

        let (_, cells) = page(&lens, "f_threads", &THREADS, rows);

        assert_eq!(cells.styles, [cell_style(0, 1, "blue")]);
    }

    const THREADS: [(&str, &str); 3] = [
        ("thread_id", "BIGINT"),
        ("title", "VARCHAR"),
        ("status", "VARCHAR"),
    ];

    fn num(value: &str) -> Cell {
        Cell::Number(value.into())
    }
}
