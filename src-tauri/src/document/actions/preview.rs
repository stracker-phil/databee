//! What an action writes, row by row, before it is queued. The previewed plan is kept in the
//! session and queued as is, so the pending change is exactly what the preview showed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use ts_rs::TS;

use crate::db::ResultSet;
use crate::db::changes::Plan;
use crate::db::where_changes::{RowsDelete, RowsUpdate};
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;
use crate::value::{Provenance, ResultValues, RowKey, Value, row_keys};

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActionPreview {
    /// Queues this plan with `queue_action_preview`.
    #[ts(type = "number")]
    pub id: u64,
    pub description: String,
    pub can_undo: bool,
    /// The rows an update or delete writes; empty for free SQL.
    pub rows: Vec<PreviewRow>,
    /// The statements of free SQL, whose rows are not known in advance; empty otherwise.
    pub sql: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub table: String,
    pub key: RowKey,
    pub change: RowChange,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RowChange {
    Update { columns: Vec<ColumnWrite> },
    Delete,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnWrite {
    pub column: String,
    pub old: Value,
    pub new: Value,
}

/// A planned action and what it writes.
pub struct Planned {
    pub plan: Plan,
    pub rows: Vec<PreviewRow>,
    pub sql: Vec<String>,
}

/// `befores[i]` and `new_values[i]` belong to `updates[i]`; a step that matched no rows has none.
pub fn updated_rows(
    updates: &[RowsUpdate],
    befores: &[ResultSet],
    new_values: &[Vec<Value>],
) -> Vec<PreviewRow> {
    let mut rows = Vec::new();
    for ((update, before), news) in updates.iter().zip(befores).zip(new_values) {
        let keys = row_keys(before, &update.key_columns);
        let values = ResultValues::new(before);
        for (r, key) in keys.into_iter().enumerate() {
            let columns = update
                .set
                .iter()
                .zip(news)
                .filter_map(|((column, _), new)| {
                    let col = before
                        .columns
                        .iter()
                        .position(|c| c.name.eq_ignore_ascii_case(column))?;
                    let source = Provenance {
                        table: update.table.clone(),
                        column: column.clone(),
                        row: key.clone(),
                    };
                    Some(ColumnWrite {
                        column: column.clone(),
                        old: values.value(r, col, Some(source)),
                        new: new.clone(),
                    })
                })
                .collect();
            rows.push(PreviewRow {
                table: update.table.clone(),
                key,
                change: RowChange::Update { columns },
            });
        }
    }
    rows
}

pub fn deleted_rows(delete: &RowsDelete, rows: &ResultSet) -> Vec<PreviewRow> {
    row_keys(rows, &delete.key_columns)
        .into_iter()
        .map(|key| PreviewRow {
            table: delete.table.clone(),
            key,
            change: RowChange::Delete,
        })
        .collect()
}

/// The latest preview of a session; previewing again replaces it.
#[derive(Default)]
pub struct Previews {
    next_id: AtomicU64,
    latest: Mutex<Option<(u64, Plan)>>,
}

impl Previews {
    pub fn keep(&self, planned: Planned) -> ActionPreview {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let preview = ActionPreview {
            id,
            description: planned.plan.description.clone(),
            can_undo: !planned.plan.undo.is_empty(),
            rows: planned.rows,
            sql: planned.sql,
        };
        *self.latest.lock_unpoisoned() = Some((id, planned.plan));
        preview
    }

    /// The plan of preview `id`, once; an older preview is refused.
    pub fn take(&self, id: u64) -> AppResult<Plan> {
        let mut latest = self.latest.lock_unpoisoned();
        match latest.take() {
            Some((kept, plan)) if kept == id => Ok(plan),
            other => {
                *latest = other;
                Err(AppError::InvalidInput(
                    "The preview is out of date; preview the action again".into(),
                ))
            }
        }
    }
}
