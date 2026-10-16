//! The task endpoints ([contracts 9], tasks). The `Task` record is in
//! [`crate::records::task`].

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::DomainError,
    id::{GardenId, PlantId, ScanId},
    records::task::{Repeat, TaskSource, TaskState},
    wire::LocalDate,
};

/// `GET /tasks?garden_id=&source=&state=`: by `due` (tasks without one last), then
/// `created_at`. `?source=forecast_alert` is the alert inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// Filter by source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TaskSource>,
    /// Filter by state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<TaskState>,
}

/// `POST /tasks`: `source: "user"`. A `plant_id` or `scan_id` that isn't live in the garden
/// gets `404`, checked in the insert's transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// A live plant of the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A live scan of the garden (and of that plant): the jar-test task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub scan_id: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// Due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// Repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
}

/// `PATCH /tasks/{id}`. Only `user` and `chat` tasks can change `title`, `detail`, `due` and
/// `repeat`; completing a repeating task creates the next one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTaskRequest {
    /// Complete or reopen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub done: Option<bool>,
    /// Dismiss or restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed: Option<bool>,
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub title: Option<String>,
    /// New detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// New due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// New repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
}

impl UpdateTaskRequest {
    /// Whether the patch edits content, which only `user` and `chat` tasks allow.
    pub fn edits_content(&self) -> bool {
        self.title.is_some() || self.detail.is_some() || self.due.is_some() || self.repeat.is_some()
    }

    /// Checks the patch against the task's source.
    pub fn check(&self, source: TaskSource) -> Result<(), DomainError> {
        if self.edits_content() && !source.editable() {
            Err(DomainError::invalid(
                "title",
                "plan and forecast-alert tasks can only be completed or dismissed",
            ))
        } else {
            Ok(())
        }
    }
}

impl Repeat {
    /// The next occurrence's due date after completing one due on `due`, or `None` past
    /// `until`.
    pub fn next_due(&self, due: LocalDate) -> Option<LocalDate> {
        let next = due.plus_days(self.every_days.max(1))?;
        match self.until {
            Some(until) if next > until => None,
            _ => Some(next),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats() {
        let r = Repeat {
            every_days: 3,
            until: Some("2026-10-25".parse().unwrap()),
        };
        assert_eq!(
            r.next_due("2026-10-20".parse().unwrap()),
            Some("2026-10-23".parse().unwrap())
        );
        assert_eq!(r.next_due("2026-10-23".parse().unwrap()), None);
    }

    #[test]
    fn only_user_and_chat_tasks_are_edited() {
        let patch = UpdateTaskRequest {
            title: Some("x".to_owned()),
            ..UpdateTaskRequest::default()
        };
        assert!(patch.check(TaskSource::User).is_ok());
        assert!(patch.check(TaskSource::Plan).is_err());
        let done = UpdateTaskRequest {
            done: Some(true),
            ..UpdateTaskRequest::default()
        };
        assert!(done.check(TaskSource::ForecastAlert).is_ok());
    }
}
