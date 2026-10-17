//! Actions: writes a preset offers on rows or tables. Structured effects (`update`, `delete`)
//! get undo from a before-image the core captures while planning; free `sql` has none.

use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::placeholder::Allowed;
use super::queries::checked_field;
use super::types::{Check, Conditions, FieldError};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum ActionScope {
    /// Offered on one row; `{id}` is its primary key.
    #[default]
    Row,
    /// Offered on the table and as an anomaly `fix`; selects its rows by `where`.
    Table,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    #[default]
    Text,
    Int,
}

/// A value asked for before the action plans; `{param.<name>}` renders it as an SQL literal
/// (NULL when empty).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    /// Offers the first column of these rows as choices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices_sql: Option<String>,
    /// Allows a value outside the choices.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
    #[serde(default, skip_serializing_if = "is_text")]
    pub r#type: ParamType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

fn is_text(t: &ParamType) -> bool {
    *t == ParamType::Text
}

/// A `set` value: text as an SQL literal, or `{ sql = "…" }` as an SQL expression.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(untagged)]
pub enum SetValue {
    Text(String),
    Sql { sql: String },
}

impl<'de> Deserialize<'de> for SetValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match toml::Value::deserialize(deserializer)? {
            toml::Value::String(text) => Ok(SetValue::Text(text)),
            toml::Value::Integer(i) => Ok(SetValue::Text(i.to_string())),
            toml::Value::Table(t) if t.len() == 1 => match t.get("sql") {
                Some(toml::Value::String(sql)) => Ok(SetValue::Sql { sql: sql.clone() }),
                _ => Err(D::Error::custom("expected { sql = \"<expression>\" }")),
            },
            _ => Err(D::Error::custom(
                "expected a string, an integer or { sql = \"<expression>\" }",
            )),
        }
    }
}

/// One `UPDATE`: `table` defaults to the action's, `key` to its primary key, matched to `{id}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct UpdateStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#where: Option<String>,
    pub set: BTreeMap<String, SetValue>,
}

/// Rows are selected first and deleted by primary key, so undo can re-insert them.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct DeleteSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#where: Option<String>,
}

/// What an action writes.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionEffect {
    Update(Vec<UpdateStep>),
    Delete(DeleteSpec),
    Sql {
        sql: String,
        preview_sql: Option<String>,
    },
}

/// A write offered on `table`, on one row or on the rows where `when` holds.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[serde(try_from = "ActionFields", into = "ActionFields")]
#[ts(export, export_to = "src/api/types.gen.ts", as = "ActionFields")]
pub struct Action {
    pub name: String,
    pub table: String,
    pub scope: ActionScope,
    pub when: Conditions,
    pub params: Vec<Param>,
    pub effect: ActionEffect,
}

/// The stored shape of an `Action`: exactly one of `update`, `delete`, `sql`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct ActionFields {
    pub name: String,
    pub table: String,
    #[serde(default, skip_serializing_if = "is_row")]
    pub scope: ActionScope,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: Conditions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub update: Vec<UpdateStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<DeleteSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_sql: Option<String>,
}

fn is_row(scope: &ActionScope) -> bool {
    *scope == ActionScope::Row
}

impl TryFrom<ActionFields> for Action {
    type Error = String;

    fn try_from(f: ActionFields) -> Result<Self, String> {
        if f.preview_sql.is_some() && f.sql.is_none() {
            return Err("`preview_sql` goes with `sql`".into());
        }
        let effect = match (f.update.is_empty(), f.delete, f.sql) {
            (false, None, None) => ActionEffect::Update(f.update),
            (true, Some(delete), None) => ActionEffect::Delete(delete),
            (true, None, Some(sql)) => ActionEffect::Sql {
                sql,
                preview_sql: f.preview_sql,
            },
            _ => return Err("an action sets exactly one of update, delete, sql".into()),
        };
        Ok(Self {
            name: f.name,
            table: f.table,
            scope: f.scope,
            when: f.when,
            params: f.params,
            effect,
        })
    }
}

impl From<Action> for ActionFields {
    fn from(a: Action) -> Self {
        let mut fields = ActionFields {
            name: a.name,
            table: a.table,
            scope: a.scope,
            when: a.when,
            params: a.params,
            update: Vec::new(),
            delete: None,
            sql: None,
            preview_sql: None,
        };
        match a.effect {
            ActionEffect::Update(steps) => fields.update = steps,
            ActionEffect::Delete(delete) => fields.delete = Some(delete),
            ActionEffect::Sql { sql, preview_sql } => {
                fields.sql = Some(sql);
                fields.preview_sql = preview_sql;
            }
        }
        fields
    }
}

impl Check for Action {
    fn check(&self) -> Result<(), FieldError> {
        let names: Vec<String> = self.params.iter().map(|p| p.name.clone()).collect();
        if let Some(dup) = names
            .iter()
            .enumerate()
            .find(|(i, n)| names[..*i].contains(n))
        {
            return Err(FieldError::new(
                "params",
                format!("`{}` is declared twice", dup.1),
            ));
        }
        let row = self.scope == ActionScope::Row;
        let allowed = Allowed {
            id: row,
            owner: false,
            meta: false,
            params: Some(&names),
        };
        for (i, param) in self.params.iter().enumerate() {
            if let Some(sql) = &param.choices_sql {
                checked_field(&format!("params[{i}].choices_sql"), sql, Allowed::default())?;
            }
        }
        match &self.effect {
            ActionEffect::Update(steps) => check_updates(steps, row, allowed),
            ActionEffect::Delete(delete) => match (&delete.r#where, row) {
                (None, false) => Err(FieldError::new(
                    "delete.where",
                    "a table action selects its rows by `where`",
                )),
                (Some(w), _) => checked_field("delete.where", w, allowed),
                (None, true) => Ok(()),
            },
            ActionEffect::Sql { sql, preview_sql } => {
                checked_field("sql", sql, allowed)?;
                preview_sql.as_ref().map_or(Ok(()), |preview| {
                    checked_field("preview_sql", preview, allowed)
                })
            }
        }
    }
}

fn check_updates(steps: &[UpdateStep], row: bool, allowed: Allowed<'_>) -> Result<(), FieldError> {
    for (i, step) in steps.iter().enumerate() {
        if step.set.is_empty() {
            return Err(FieldError::new(format!("update[{i}].set"), "sets nothing"));
        }
        if !row && step.r#where.is_none() {
            return Err(FieldError::new(
                format!("update[{i}].where"),
                "a table action selects its rows by `where`",
            ));
        }
        if let Some(w) = &step.r#where {
            checked_field(&format!("update[{i}].where"), w, allowed)?;
        }
        for (column, value) in &step.set {
            let text = match value {
                SetValue::Text(t) => t,
                SetValue::Sql { sql } => sql,
            };
            checked_field(&format!("update[{i}].set.{column}"), text, allowed)?;
        }
    }
    Ok(())
}
