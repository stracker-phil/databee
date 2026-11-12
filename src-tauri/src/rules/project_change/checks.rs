//! The checks a change passes before it is stored: rule tables and columns exist, ids are free.

use std::collections::HashSet;
use std::sync::Arc;

use super::ChangeContext;
use crate::rules::applied::Applied;
use crate::rules::icons::IconRule;
use crate::rules::kinds::StyleRule;
use crate::rules::project::Project;
use crate::rules::types::{Check, ColumnGlob, Conditions, FieldError, Preset, Reference};
use crate::schema::DatabaseSchema;

/// `field` prefixed to a rule's own field error (`reference.label`).
fn nested(section: &str, error: FieldError) -> FieldError {
    FieldError::new(format!("{section}.{}", error.field), error.message)
}

/// A style without an id gets its default id, or `-><color>` when the project already uses that.
pub(super) fn new_style_id(styles: &[StyleRule], rule: &StyleRule) -> Result<String, FieldError> {
    if styles.iter().any(|s| s.same_style(rule)) {
        let message = "the project already has this value style";
        return Err(FieldError::new("style.value", message));
    }
    let taken = |id: &str| styles.iter().any(|s| s.id == id);
    if !rule.id.is_empty() {
        if taken(&rule.id) {
            let message = format!("`{}` is already used in this project", rule.id);
            return Err(FieldError::new("style.id", message));
        }
        return Ok(rule.id.clone());
    }
    let default = rule.default_id();
    if !taken(&default) {
        return Ok(default);
    }
    let qualified = rule.qualified_id();
    if taken(&qualified) {
        let message = format!("`{qualified}` is already used; give this value style an `id`");
        return Err(FieldError::new("style.id", message));
    }
    Ok(qualified)
}

pub(super) fn check_rule_id(id: &str) -> Result<(), FieldError> {
    if id.is_empty() {
        return Err(FieldError::new("id", "is empty"));
    }
    Ok(())
}

/// A prefix no table starts with is a typo: no rule could resolve under it.
pub(super) fn check_prefix(
    schema: &DatabaseSchema,
    field: &str,
    prefix: Option<&str>,
) -> Result<(), FieldError> {
    let Some(prefix) = prefix else { return Ok(()) };
    if schema.tables.iter().any(|t| t.name.starts_with(prefix)) {
        return Ok(());
    }
    let message = format!("no table starts with `{prefix}`");
    Err(FieldError::new(field, message))
}

pub(super) fn check_index(index: usize, len: usize) -> Result<(), FieldError> {
    if index >= len {
        let message = format!("{index} is out of range; the project has {len} such rules");
        return Err(FieldError::new("index", message));
    }
    Ok(())
}

pub(super) fn reference_index(references: &[Reference], id: &str) -> Result<usize, FieldError> {
    references
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| FieldError::new("id", format!("the project has no reference `{id}`")))
}

/// The id `reference` is stored under; `replacing` is the index it takes over, whose id it
/// keeps when none is given. A default id that is taken gets `-><to>`, as in presets.
pub(super) fn new_reference_id(
    references: &[Reference],
    reference: &Reference,
    replacing: Option<usize>,
) -> Result<String, FieldError> {
    let taken = |id: &str| {
        references
            .iter()
            .enumerate()
            .any(|(i, r)| Some(i) != replacing && r.id == id)
    };
    if !reference.id.is_empty() {
        if taken(&reference.id) {
            let message = format!("`{}` is already used in this project", reference.id);
            return Err(FieldError::new("reference.id", message));
        }
        return Ok(reference.id.clone());
    }
    if let Some(index) = replacing {
        return Ok(references[index].id.clone());
    }
    let default = reference.default_id();
    if !taken(&default) {
        return Ok(default);
    }
    let qualified = reference.qualified_id();
    if taken(&qualified) {
        let message = format!("`{qualified}` is already used; give this reference an `id`");
        return Err(FieldError::new("reference.id", message));
    }
    Ok(qualified)
}

pub(super) fn check_presets(
    names: Option<&[String]>,
    presets: &[Arc<Preset>],
) -> Result<(), FieldError> {
    let Some(names) = names else { return Ok(()) };
    let mut seen = HashSet::new();
    for (i, name) in names.iter().enumerate() {
        let field = format!("presets[{i}]");
        if !presets.iter().any(|p| &p.name == name) {
            let message = format!("no preset `{name}` loads (see the presets list)");
            return Err(FieldError::new(field, message));
        }
        if !seen.insert(name) {
            return Err(FieldError::new(field, format!("`{name}` is listed twice")));
        }
    }
    Ok(())
}

/// Resolves rule table names (written without a prefix) against the schema as the effective
/// rules do (`Applied::existing_table`), so a rule that passes here checks the table it applies to.
pub(super) struct Tables<'a> {
    schema: &'a DatabaseSchema,
    applied: Applied,
}

impl<'a> Tables<'a> {
    pub(super) fn new(project: &Project, ctx: &'a ChangeContext) -> Self {
        let schema = ctx.schema.as_ref();
        Self {
            schema,
            applied: Applied::new(project, schema, &ctx.presets),
        }
    }

    /// The real table `name` stands for, under the prefix or the network prefix.
    fn resolve(&self, field: &str, name: &str) -> Result<String, FieldError> {
        if let Some(found) = self.applied.existing_table(name) {
            return Ok(found);
        }
        let prefixed = format!("{}{name}", self.applied.prefix);
        let networked = format!("{}{name}", self.applied.network_prefix);
        let message = if prefixed == networked {
            format!("no table `{prefixed}` in this database")
        } else {
            format!("no table `{prefixed}` or `{networked}` in this database")
        };
        Err(FieldError::new(field, message))
    }

    fn check_column(&self, field: &str, table: &str, column: &str) -> Result<(), FieldError> {
        let real = self.resolve(field, table)?;
        if self.schema.columns(&real).iter().any(|c| c == column) {
            return Ok(());
        }
        let message = format!("`{real}` has no column `{column}`");
        Err(FieldError::new(field, message))
    }

    fn check_conditions(
        &self,
        field: &str,
        table: &str,
        when: &Conditions,
    ) -> Result<(), FieldError> {
        for column in when.keys() {
            self.check_column(&format!("{field}.{column}"), table, column)?;
        }
        Ok(())
    }

    pub(super) fn check_reference(&self, reference: &Reference) -> Result<(), FieldError> {
        reference.check().map_err(|e| nested("reference", e))?;
        let (from, to) = (&reference.from, &reference.to);
        self.check_column("reference.from", &from.table, &from.column)?;
        self.check_conditions("reference.when", &reference.from.table, &reference.when)?;
        self.check_column("reference.to", &to.table, &to.column)?;
        self.check_conditions("reference.to_when", &reference.to.table, &reference.to_when)
    }

    /// Globs match whatever exists, so only the literal parts are checked.
    pub(super) fn check_globs(
        &self,
        section: &str,
        columns: &[ColumnGlob],
        in_meta: &[String],
    ) -> Result<(), FieldError> {
        for (i, glob) in columns.iter().enumerate() {
            let field = format!("{section}.columns[{i}]");
            if glob.table.contains('*') {
                continue;
            }
            if glob.column.contains('*') {
                self.resolve(&field, &glob.table)?;
            } else {
                self.check_column(&field, &glob.table, &glob.column)?;
            }
        }
        for (i, table) in in_meta.iter().enumerate() {
            self.resolve(&format!("{section}.in[{i}]"), table)?;
        }
        Ok(())
    }

    /// A glob matches whatever exists; a name must resolve.
    pub(super) fn check_icon(&self, rule: &IconRule) -> Result<(), FieldError> {
        rule.check().map_err(|e| nested("icon", e))?;
        if !rule.is_glob() {
            self.resolve("icon.table", &rule.table)?;
        }
        Ok(())
    }

    pub(super) fn check_style(&self, rule: &StyleRule) -> Result<(), FieldError> {
        rule.check().map_err(|e| nested("style", e))?;
        self.check_globs("style", &rule.columns, &rule.r#in)
    }
}
