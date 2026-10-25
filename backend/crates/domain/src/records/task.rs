//! `Task` ([contracts 7], task) and the server-side alert key.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{GardenId, PlanId, PlantId, ScanId, StepId, TaskId},
    records::plan::TriggerKind,
    wire::{LocalDate, Timestamp},
};

/// `Task.repeat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Repeat {
    /// At least 1.
    pub every_days: u32,
    /// The last date an occurrence is created for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub until: Option<LocalDate>,
}

/// `Task.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    /// A plan step; dismissed, never deleted.
    Plan,
    /// A forecast alert; dismissed, never deleted.
    ForecastAlert,
    /// A confirmed `create_task` card.
    Chat,
    /// `POST /tasks`.
    User,
}

impl TaskSource {
    /// `user` and `chat` tasks can be edited and deleted; the others only completed or
    /// dismissed, so the scheduler never recreates one the user removed ([contracts 7]).
    pub const fn editable(self) -> bool {
        matches!(self, Self::User | Self::Chat)
    }
}

/// `Task.trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskTrigger {
    /// The rule that fired.
    pub kind: TriggerKind,
    /// The local date it fired for.
    pub date: LocalDate,
}

/// `Task.dismissed_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DismissedReason {
    /// The user dismissed it.
    User,
    /// Its plan was superseded.
    PlanSuperseded,
}

/// `Task` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Task {
    /// The task.
    pub id: TaskId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant, always a live one when created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// The plan version that made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// The step that made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_step_id: Option<StepId>,
    /// A guided-check task's scan: the jar test photographed later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub scan_id: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// Due date, local to the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// Completing it creates the next occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
    /// Who made it.
    pub source: TaskSource,
    /// The rule that created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub trigger: Option<TaskTrigger>,
    /// When it was completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub done_at: Option<Timestamp>,
    /// When it was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed_at: Option<Timestamp>,
    /// Why it was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed_reason: Option<DismissedReason>,
    /// When it was created.
    pub created_at: Timestamp,
}

/// `?state=open|done|dismissed`, stored as `task.state` ([database 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Neither done nor dismissed.
    Open,
    /// Completed.
    Done,
    /// Dismissed.
    Dismissed,
}

impl Task {
    /// The task's state, as the schema computes it.
    pub fn state(&self) -> TaskState {
        if self.done_at.is_some() {
            TaskState::Done
        } else if self.dismissed_at.is_some() {
            TaskState::Dismissed
        } else {
            TaskState::Open
        }
    }
}

/// `task.alert_key`, server-side only, under one unique index ([contracts 7]):
/// `<garden_id>:<trigger>:<date>` for forecast alerts and
/// `plan:<plan_id>:<plan_step_id>:<date>` for plan triggers. A second insert with the same key
/// fails, which makes the scheduler's dedupe race-free ([database 4], alert dedupe).
///
/// The date is the first day of the alert's spell: the consecutive days on which the rule
/// fires alert once ([architecture 8.3]). The key without its date, [`spell`](Self::spell),
/// is stored as `task.spell` beside how far the spell has been seen (`task.spell_until`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlertKey {
    /// A forecast alert.
    Forecast {
        /// The garden.
        garden: GardenId,
        /// The rule.
        trigger: TriggerKind,
        /// The local date.
        date: LocalDate,
    },
    /// A plan trigger rule.
    PlanTrigger {
        /// The plan version.
        plan: PlanId,
        /// The step.
        step: StepId,
        /// The local date.
        date: LocalDate,
    },
}

impl AlertKey {
    /// `task.spell`: the key without its date, `<garden_id>:<trigger>` or
    /// `plan:<plan_id>:<plan_step_id>`, shared by every spell of the rule.
    pub fn spell(&self) -> String {
        match self {
            Self::Forecast {
                garden, trigger, ..
            } => format!("{garden}:{}", trigger.as_str()),
            Self::PlanTrigger { plan, step, .. } => format!("plan:{plan}:{step}"),
        }
    }

    /// The spell's first day.
    pub const fn date(&self) -> LocalDate {
        match self {
            Self::Forecast { date, .. } | Self::PlanTrigger { date, .. } => *date,
        }
    }
}

impl fmt::Display for AlertKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forecast {
                garden,
                trigger,
                date,
            } => write!(f, "{garden}:{}:{date}", trigger.as_str()),
            Self::PlanTrigger { plan, step, date } => write!(f, "plan:{plan}:{step}:{date}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_keys() {
        let date: LocalDate = "2026-10-20".parse().unwrap();
        let key = AlertKey::Forecast {
            garden: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            trigger: TriggerKind::Frost,
            date,
        };
        assert_eq!(
            key.to_string(),
            "01JAX3K9T6R2B8V0QZ4M7D1E5F:frost:2026-10-20"
        );
        assert_eq!(key.spell(), "01JAX3K9T6R2B8V0QZ4M7D1E5F:frost");
        assert_eq!(key.date(), date);
        let key = AlertKey::PlanTrigger {
            plan: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            step: "s3".parse().unwrap(),
            date,
        };
        assert_eq!(
            key.to_string(),
            "plan:01JAX3K9T6R2B8V0QZ4M7D1E5F:s3:2026-10-20"
        );
        assert_eq!(key.spell(), "plan:01JAX3K9T6R2B8V0QZ4M7D1E5F:s3");
        assert_eq!(key.date(), date);
    }
}
