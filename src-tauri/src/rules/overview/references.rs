//! The reference rows of the Project page: project and preset rules, foreign keys and inferred
//! references, each with its origin.

use std::collections::HashSet;

use super::rows::{ReferenceRow, RuleOrigin};
use super::{Overview, preset_origin};
use crate::lens::{ColumnsRef, Lens, ResolvedReference, SchemaLens};
use crate::rules::condition::conditions_text;
use crate::rules::effective::{inferred_id, overrides};
use crate::rules::types::{ColumnRef, Conditions, Reference};
use crate::value::ReferenceSource;

impl Overview<'_> {
    pub(super) fn real(&self, column: &ColumnRef) -> Option<ColumnsRef> {
        let table = self.rules.applied.existing_table(&column.table)?;
        Some(ColumnsRef::new(&table, vec![column.column.clone()]))
    }

    /// A row for a stored rule; `None` when its tables do not exist here.
    fn rule_row(
        &self,
        rule: &Reference,
        origin: RuleOrigin,
        by: ReferenceSource,
        edited: bool,
    ) -> Option<ReferenceRow> {
        let resolved = ResolvedReference {
            from: self.real(&rule.from)?,
            to: self.real(&rule.to)?,
            by,
        };
        let reference = self.active.contains(&resolved).then(|| resolved.clone());
        let ResolvedReference { from, to, .. } = resolved;
        Some(ReferenceRow {
            origin,
            id: Some(rule.id.clone()),
            from,
            to,
            when: conditions_text(&rule.when),
            to_when: conditions_text(&rule.to_when),
            label: rule.label.clone(),
            disabled: self.rules.disabled.contains(&rule.id),
            edited,
            reference,
            rule: Some(rule.clone()),
        })
    }

    /// Project rules (each in the row of the preset or inferred reference it replaces), then the
    /// preset rules no project rule replaces (last applied preset first), then foreign keys and
    /// inferred references.
    pub(super) fn references(&self) -> Vec<ReferenceRow> {
        let presets: Vec<_> = self.rules.preset_reference_rules(self.schema).collect();
        let inferred = self.inferred_references();
        let mut replaced = HashSet::new();
        let mut rows = Vec::new();
        for own in &self.project.references {
            let by = ReferenceSource::Project { id: own.id.clone() };
            let preset = presets
                .iter()
                .find(|(_, rules)| rules.iter().any(|r| overrides(own, r)));
            let origin = match (preset, self.replaced_inference(own, &inferred)) {
                (Some((preset, _)), _) => preset_origin(preset),
                (None, Some((id, indexed))) => {
                    replaced.insert(id);
                    RuleOrigin::Inferred { indexed }
                }
                (None, None) => RuleOrigin::Project,
            };
            let edited = origin != RuleOrigin::Project;
            rows.extend(self.rule_row(own, origin, by, edited));
        }
        for (preset, rules) in &presets {
            let kept = rules
                .iter()
                .filter(|r| !self.project.references.iter().any(|own| overrides(own, r)));
            for rule in kept {
                let by = ReferenceSource::Preset {
                    preset: preset.name.clone(),
                    id: rule.id.clone(),
                };
                rows.extend(self.rule_row(rule, preset_origin(preset), by, false));
            }
        }
        rows.extend(self.schema_references(&replaced));
        rows
    }

    fn schema_lens(&self) -> SchemaLens {
        SchemaLens::new(
            self.schema.clone(),
            &self.project.connection,
            &self.project.database,
        )
    }

    pub(super) fn inferred_references(&self) -> Vec<ResolvedReference> {
        let schema_lens = self.schema_lens();
        self.schema
            .tables
            .iter()
            .flat_map(|t| schema_lens.references_from(&t.name))
            .filter(|r| matches!(r.by, ReferenceSource::SameName { .. }))
            .collect()
    }

    /// The inferred id and `indexed` of the same-name inference an unconditional project rule
    /// replaces on its column.
    fn replaced_inference(
        &self,
        own: &Reference,
        inferred: &[ResolvedReference],
    ) -> Option<(String, bool)> {
        if !own.when.is_empty() {
            return None;
        }
        let from = self.real(&own.from)?;
        inferred.iter().find(|r| r.from == from).and_then(|r| {
            let ReferenceSource::SameName { indexed, .. } = r.by else {
                return None;
            };
            Some((inferred_column_id(r)?, indexed))
        })
    }

    /// Foreign keys and inferred references, in one pass over what the schema alone gives: an
    /// inferred reference the project switched off is listed as disabled, one a rule replaces is
    /// left out (or listed by the project rule that edits it, `replaced`).
    fn schema_references(&self, replaced: &HashSet<String>) -> Vec<ReferenceRow> {
        let schema_lens = self.schema_lens();
        self.schema
            .tables
            .iter()
            .flat_map(|t| schema_lens.references_from(&t.name))
            .filter_map(|reference| match &reference.by {
                ReferenceSource::ForeignKey { constraint } => {
                    let origin = RuleOrigin::ForeignKey {
                        constraint: constraint.clone(),
                    };
                    Some(self.schema_row(reference, origin))
                }
                ReferenceSource::SameName { indexed, .. } => {
                    let origin = RuleOrigin::Inferred { indexed: *indexed };
                    let id = inferred_column_id(&reference)?;
                    if replaced.contains(&id) {
                        return None;
                    }
                    let disabled = self.rules.disabled.contains(&id);
                    let applies = self.active.contains(&reference);
                    (disabled || applies).then(|| self.schema_row(reference, origin))
                }
                _ => None,
            })
            .collect()
    }

    fn schema_row(&self, reference: ResolvedReference, origin: RuleOrigin) -> ReferenceRow {
        let (id, rule) = match origin {
            RuleOrigin::Inferred { .. } => {
                (inferred_column_id(&reference), self.replacement(&reference))
            }
            _ => (None, None),
        };
        let applies = self.active.contains(&reference);
        ReferenceRow {
            origin,
            id,
            from: reference.from.clone(),
            to: reference.to.clone(),
            when: String::new(),
            to_when: String::new(),
            label: None,
            disabled: !applies,
            edited: false,
            reference: applies.then_some(reference),
            rule,
        }
    }

    /// The project rule that would replace the inferred `reference` (an empty id: it gets its
    /// default when stored).
    fn replacement(&self, reference: &ResolvedReference) -> Option<Reference> {
        let ([from], [to]) = (
            reference.from.columns.as_slice(),
            reference.to.columns.as_slice(),
        ) else {
            return None;
        };
        let applied = &self.rules.applied;
        let from_table = *applied.rule_names(&reference.from.table).first()?;
        let to_table = *applied.rule_names(&reference.to.table).first()?;
        Some(Reference {
            id: String::new(),
            from: ColumnRef {
                table: from_table.to_string(),
                column: from.clone(),
            },
            when: Conditions::new(),
            to: ColumnRef {
                table: to_table.to_string(),
                column: to.clone(),
            },
            to_when: Conditions::new(),
            label: None,
            columns: Vec::new(),
        })
    }
}

fn inferred_column_id(reference: &ResolvedReference) -> Option<String> {
    let [column] = reference.from.columns.as_slice() else {
        return None;
    };
    Some(inferred_id(&reference.from.table, column))
}
