//! Push notifications ([contracts 9], push notifications).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{GardenId, PlantId, PushToken, ScanId, TaskId},
    records::plan::TriggerKind,
    wire::V1,
};

/// `PushPayload.reason` for `kind: "scan"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PushScanReason {
    /// A run reached `planned`.
    PlanReady,
    /// A late answer, a guided check or a species change produced a new plan version (not for
    /// `chat_revision`).
    PlanSuperseded,
    /// A run asked a question.
    Question,
    /// The scan failed.
    Failed,
}

/// `PushPayload` ([contracts 9]): the `data` of an Expo push message, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PushPayload {
    /// To the scan's creator, while still a member.
    Scan {
        /// `1`.
        v: V1,
        /// Why.
        reason: PushScanReason,
        /// The garden.
        garden_id: GardenId,
        /// The plant.
        plant_id: PlantId,
        /// The scan.
        scan_id: ScanId,
    },
    /// A forecast alert's task, to every member.
    Alert {
        /// `1`.
        v: V1,
        /// The garden.
        garden_id: GardenId,
        /// The alert task.
        task_id: TaskId,
        /// The rule.
        trigger: TriggerKind,
    },
    /// To every other member the garden had.
    GardenDeleted {
        /// `1`.
        v: V1,
        /// Its name.
        garden_name: String,
    },
}

/// One push message for the `Notifier`: a short English title and body, and the payload as
/// `data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMessage {
    /// The device token (iOS or Android).
    pub to: PushToken,
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
    /// `data`.
    pub data: PushPayload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_shapes() {
        let p = PushPayload::GardenDeleted {
            v: V1,
            garden_name: "Back yard".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({"kind": "garden_deleted", "v": 1, "garden_name": "Back yard"})
        );
    }
}
