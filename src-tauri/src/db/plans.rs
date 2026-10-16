//! Planned writes kept in the session and applied by id, so the page never sends SQL to
//! execute (ADR 0012). Applied batches keep their undo statements for undo by batch.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use super::changes::Plan;
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

pub type PlanId = u64;
pub type BatchId = u64;

/// Undo history depth; older batches can no longer be undone.
const MAX_BATCHES: usize = 20;

/// What the page renders for review; `statements` are for display only.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedChange {
    pub id: PlanId,
    pub description: String,
    pub statements: Vec<String>,
    pub can_undo: bool,
}

/// What the page learns about an applied batch.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedBatchInfo {
    pub batch_id: BatchId,
    pub affected: u64,
    pub can_undo: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedBatch {
    pub id: BatchId,
    /// Undo statements, last applied plan first.
    pub undo: Vec<String>,
    /// Every database the batch wrote to.
    pub databases: Vec<String>,
    /// Every plan of the batch could be undone.
    pub can_undo: bool,
}

#[derive(Default)]
pub struct Plans {
    next_id: AtomicU64,
    pending: Mutex<Vec<(PlanId, Plan)>>,
    applied: Mutex<VecDeque<AppliedBatch>>,
}

impl Plans {
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn add(&self, plan: Plan) -> PlannedChange {
        let id = self.next_id();
        let change = PlannedChange {
            id,
            description: plan.description.clone(),
            statements: plan.statements.clone(),
            can_undo: !plan.undo.is_empty(),
        };
        self.pending.lock_unpoisoned().push((id, plan));
        change
    }

    pub fn discard(&self, id: PlanId) {
        self.pending.lock_unpoisoned().retain(|(p, _)| *p != id);
    }

    pub fn discard_all(&self) {
        self.pending.lock_unpoisoned().clear();
    }

    /// Removes the plans in the given order; with any unknown id nothing is removed.
    pub fn take(&self, ids: &[PlanId]) -> AppResult<Vec<(PlanId, Plan)>> {
        let mut pending = self.pending.lock_unpoisoned();
        if let Some(missing) = ids.iter().find(|id| !pending.iter().any(|(p, _)| p == *id)) {
            return Err(AppError::InvalidInput(format!(
                "Planned change #{missing} is no longer pending"
            )));
        }
        let mut taken = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(pos) = pending.iter().position(|(p, _)| p == id) {
                taken.push(pending.remove(pos));
            }
        }
        Ok(taken)
    }

    /// Puts plans back after a failed apply, so the user can retry them.
    pub fn restore(&self, plans: Vec<(PlanId, Plan)>) {
        self.pending.lock_unpoisoned().extend(plans);
    }

    /// Records an applied batch for undo; the oldest batch beyond `MAX_BATCHES` is dropped.
    pub fn record_applied(&self, plans: &[(PlanId, Plan)]) -> AppliedBatch {
        let mut databases: Vec<String> = Vec::new();
        for (_, plan) in plans {
            if !databases.contains(&plan.database) {
                databases.push(plan.database.clone());
            }
        }
        let batch = AppliedBatch {
            id: self.next_id(),
            undo: plans
                .iter()
                .rev()
                .flat_map(|(_, plan)| plan.undo.iter().cloned())
                .collect(),
            databases,
            can_undo: plans.iter().all(|(_, plan)| !plan.undo.is_empty()),
        };
        let mut applied = self.applied.lock_unpoisoned();
        applied.push_back(batch.clone());
        while applied.len() > MAX_BATCHES {
            applied.pop_front();
        }
        batch
    }

    /// Removes a batch from the undo history; unknown, already undone or not undoable
    /// batches are an error.
    pub fn take_undo(&self, id: BatchId) -> AppResult<AppliedBatch> {
        let mut applied = self.applied.lock_unpoisoned();
        let pos = applied.iter().position(|b| b.id == id).ok_or_else(|| {
            AppError::InvalidInput(format!("Batch #{id} can no longer be undone"))
        })?;
        if !applied[pos].can_undo {
            return Err(AppError::InvalidInput(format!(
                "Batch #{id} has changes without undo"
            )));
        }
        Ok(applied.remove(pos).expect("position is in range"))
    }

    /// Puts a batch back after a failed undo, so the user can retry it.
    pub fn restore_undo(&self, batch: AppliedBatch) {
        self.applied.lock_unpoisoned().push_back(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(name: &str, undo: &[&str]) -> Plan {
        Plan {
            database: "wp".into(),
            description: format!("change {name}"),
            statements: vec![format!("DO {name}")],
            undo: undo.iter().map(ToString::to_string).collect(),
        }
    }

    fn taken_ids(taken: &[(PlanId, Plan)]) -> Vec<PlanId> {
        taken.iter().map(|(id, _)| *id).collect()
    }

    #[test]
    fn added_plan_is_reported_for_review_with_its_undo_availability() {
        let plans = Plans::default();

        let with_undo = plans.add(plan("a", &["UNDO a"]));
        let without_undo = plans.add(plan("b", &[]));

        assert_ne!(with_undo.id, without_undo.id);
        assert_eq!(
            serde_json::to_value(&with_undo).unwrap(),
            serde_json::json!({
                "id": with_undo.id,
                "description": "change a",
                "statements": ["DO a"],
                "canUndo": true,
            })
        );
        assert!(!without_undo.can_undo);
    }

    #[test]
    fn take_with_an_unknown_id_is_invalid_input_and_keeps_the_other_plans_pending() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &[]));
        let unknown = a.id + 100;

        let err = plans.take(&[a.id, unknown]).unwrap_err();

        assert_eq!(err.kind(), "invalid_input");
        assert_eq!(taken_ids(&plans.take(&[a.id]).unwrap()), [a.id]);
    }

    #[test]
    fn take_removes_exactly_the_given_ids_in_the_given_order() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &[]));
        let b = plans.add(plan("b", &[]));
        let c = plans.add(plan("c", &[]));

        let taken = plans.take(&[c.id, a.id]).unwrap();

        assert_eq!(taken_ids(&taken), [c.id, a.id]);
        assert_eq!(taken[0].1, plan("c", &[]));
        assert_eq!(plans.take(&[a.id]).unwrap_err().kind(), "invalid_input");
        assert_eq!(plans.take(&[c.id]).unwrap_err().kind(), "invalid_input");
        assert_eq!(taken_ids(&plans.take(&[b.id]).unwrap()), [b.id]);
    }

    #[test]
    fn discard_removes_one_plan() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &[]));
        let b = plans.add(plan("b", &[]));

        plans.discard(a.id);

        assert_eq!(plans.take(&[a.id]).unwrap_err().kind(), "invalid_input");
        assert_eq!(taken_ids(&plans.take(&[b.id]).unwrap()), [b.id]);
    }

    #[test]
    fn discard_all_removes_every_pending_plan() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &[]));
        let b = plans.add(plan("b", &[]));

        plans.discard_all();

        assert!(plans.take(&[a.id]).is_err());
        assert!(plans.take(&[b.id]).is_err());
    }

    #[test]
    fn restored_plans_can_be_taken_again() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &[]));
        let taken = plans.take(&[a.id]).unwrap();
        assert!(plans.take(&[a.id]).is_err());

        plans.restore(taken);

        assert_eq!(taken_ids(&plans.take(&[a.id]).unwrap()), [a.id]);
    }

    #[test]
    fn take_undo_returns_the_undo_statements_with_the_last_plan_first() {
        let plans = Plans::default();
        let first = plans.add(plan("first", &["UNDO first"]));
        let last = plans.add(plan("last", &["UNDO last 1", "UNDO last 2"]));
        let taken = plans.take(&[first.id, last.id]).unwrap();
        let batch = plans.record_applied(&taken);

        let undone = plans.take_undo(batch.id).unwrap();

        assert_eq!(undone.undo, ["UNDO last 1", "UNDO last 2", "UNDO first"]);
        assert_eq!(undone.databases, ["wp"]);
        assert!(undone.can_undo);
    }

    #[test]
    fn batch_lists_each_written_database_once() {
        let plans = Plans::default();
        let mut other = plan("b", &["UNDO b"]);
        other.database = "blog".into();
        let a = plans.add(plan("a", &["UNDO a"]));
        let b = plans.add(other);
        let c = plans.add(plan("c", &["UNDO c"]));
        let taken = plans.take(&[a.id, b.id, c.id]).unwrap();

        let batch = plans.record_applied(&taken);

        assert_eq!(batch.databases, ["wp", "blog"]);
    }

    #[test]
    fn a_batch_can_be_undone_only_once() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &["UNDO a"]));
        let batch = plans.record_applied(&plans.take(&[a.id]).unwrap());
        plans.take_undo(batch.id).unwrap();

        let err = plans.take_undo(batch.id).unwrap_err();

        assert_eq!(err.kind(), "invalid_input");
    }

    #[test]
    fn restored_batch_can_be_undone_again() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &["UNDO a"]));
        let batch = plans.record_applied(&plans.take(&[a.id]).unwrap());
        let undone = plans.take_undo(batch.id).unwrap();

        plans.restore_undo(undone.clone());

        assert_eq!(plans.take_undo(batch.id).unwrap(), undone);
    }

    #[test]
    fn a_batch_with_a_plan_without_undo_cannot_be_undone() {
        let plans = Plans::default();
        let a = plans.add(plan("a", &["UNDO a"]));
        let b = plans.add(plan("b", &[]));
        let batch = plans.record_applied(&plans.take(&[a.id, b.id]).unwrap());

        let err = plans.take_undo(batch.id).unwrap_err();

        assert!(!batch.can_undo);
        assert_eq!(err.kind(), "invalid_input");
    }

    #[test]
    fn only_the_last_twenty_batches_stay_undoable() {
        let plans = Plans::default();
        let batches: Vec<BatchId> = (0..21)
            .map(|n| {
                let added = plans.add(plan(&n.to_string(), &["UNDO"]));
                plans.record_applied(&plans.take(&[added.id]).unwrap()).id
            })
            .collect();

        assert_eq!(
            plans.take_undo(batches[0]).unwrap_err().kind(),
            "invalid_input"
        );
        assert!(plans.take_undo(batches[1]).is_ok());
        assert!(plans.take_undo(batches[20]).is_ok());
    }
}
