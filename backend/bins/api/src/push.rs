//! Push notifications ([contracts 9], push notifications): who gets which push. The writer sends
//! the scan pushes (it sees every event, including the broker's failures), the scheduler the
//! alert pushes, and the deletion runner the `garden_deleted` pushes. A push is a hint, never
//! the only copy, so a failed send is logged and dropped.

use std::sync::Arc;

use domain::{
    events::{ScanEventBody, SupersedeReason},
    garden::push::{PushMessage, PushPayload, PushScanReason},
    id::{GardenId, ScanId, UserId},
    ports::{Notifier, PushTarget, Store},
    url::Stored,
    wire::V1,
};

/// Sends pushes through the `Notifier` port to the tokens a user registered for iOS or Android.
#[derive(Clone)]
pub struct Pusher {
    store: Arc<dyn Store>,
    notifier: Arc<dyn Notifier>,
}

impl Pusher {
    /// A pusher over the store and the notifier.
    pub fn new(store: Arc<dyn Store>, notifier: Arc<dyn Notifier>) -> Self {
        Self { store, notifier }
    }

    /// The scan push an event calls for, if any: `plan_ready`, `plan_superseded` (except a chat
    /// revision: the member who confirmed it is looking at it), `question`, `failed`.
    pub fn scan_reason(event: &ScanEventBody<Stored>) -> Option<PushScanReason> {
        match event {
            ScanEventBody::PlanReady { .. } => Some(PushScanReason::PlanReady),
            ScanEventBody::PlanSuperseded { reason, .. }
                if *reason != SupersedeReason::ChatRevision =>
            {
                Some(PushScanReason::PlanSuperseded)
            }
            ScanEventBody::Question { .. } => Some(PushScanReason::Question),
            ScanEventBody::Failed { .. } => Some(PushScanReason::Failed),
            _ => None,
        }
    }

    /// A scan push to the scan's creator, while still a member of its garden.
    pub async fn to_scan_creator(&self, scan: ScanId, reason: PushScanReason) {
        let snapshot = match self.store.snapshot(scan).await {
            Ok(s) => s,
            Err(error) => {
                tracing::debug!(%scan, error = %error, "no push: scan unreadable");
                return;
            }
        };
        let Some(creator) = snapshot.scan.created_by.as_ref().map(|u| u.user_id) else {
            return;
        };
        let garden = snapshot.scan.garden_id;
        if !matches!(self.store.role_in(creator, garden).await, Ok(Some(_))) {
            return;
        }
        let (title, body) = match reason {
            PushScanReason::PlanReady => ("Your plan is ready", "See what to do for your plant."),
            PushScanReason::PlanSuperseded => {
                ("Your plan was updated", "Open it to see what changed.")
            }
            PushScanReason::Question => ("A quick question", "Answer it to finish the diagnosis."),
            PushScanReason::Failed => ("The scan didn't finish", "Open it to try again."),
        };
        let payload = PushPayload::Scan {
            v: V1,
            reason,
            garden_id: garden,
            plant_id: snapshot.scan.plant_id,
            scan_id: scan,
        };
        self.send(&[creator], title, body, payload).await;
    }

    /// The same push to every user in `users` (alerts: every member; `garden_deleted`: every
    /// other member). Only Expo push tokens of `ios` and `android` are sent to: a token stored
    /// before `POST /me/push-tokens` checked its form is skipped, since Expo would refuse the
    /// batch it is in. One `push sent` line per call says what happened.
    pub async fn send(&self, users: &[UserId], title: &str, body: &str, payload: PushPayload) {
        let (kind, reason) = describe(&payload);
        let targets = match self.store.push_targets(users).await {
            Ok(t) => t,
            Err(error) => {
                tracing::warn!(kind, reason, error = %error, "push targets unreadable");
                return;
            }
        };
        let deliverable: Vec<PushTarget> = targets
            .into_iter()
            .filter(|t| t.platform.deliverable())
            .collect();
        let devices = deliverable.len();
        let messages: Vec<PushMessage> = deliverable
            .into_iter()
            .filter(|t| t.token.is_expo())
            .map(|t| PushMessage {
                to: t.token,
                title: title.to_owned(),
                body: body.to_owned(),
                data: payload.clone(),
            })
            .collect();
        let sent = messages.len();
        let skipped = devices - sent;
        if messages.is_empty() {
            tracing::info!(
                kind,
                reason,
                recipients = users.len(),
                devices,
                skipped,
                messages = 0,
                "push sent"
            );
            return;
        }
        let report = match self.notifier.push(messages).await {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(kind, reason, recipients = users.len(), messages = sent, error = %error, "push not sent");
                return;
            }
        };
        let mut forgotten = 0;
        if !report.unregistered.is_empty() {
            // DeviceNotRegistered: stop sending to those tokens.
            match self.store.forget_push_tokens(&report.unregistered).await {
                Ok(()) => forgotten = report.unregistered.len(),
                Err(error) => {
                    tracing::warn!(error = %error, "unregistered push tokens not removed");
                }
            }
        }
        tracing::info!(
            kind,
            reason,
            recipients = users.len(),
            devices,
            skipped,
            messages = sent,
            accepted = report.accepted,
            unregistered = report.unregistered.len(),
            forgotten,
            failed = report.failed,
            "push sent"
        );
    }

    /// Every member of a garden.
    pub async fn members(&self, garden: GardenId) -> Vec<UserId> {
        self.store.member_ids(garden).await.unwrap_or_default()
    }
}

/// A payload's `kind` and, for a scan push, its `reason`, as the wire names them, for the log.
const fn describe(payload: &PushPayload) -> (&'static str, Option<&'static str>) {
    match payload {
        PushPayload::Scan { reason, .. } => (
            "scan",
            Some(match reason {
                PushScanReason::PlanReady => "plan_ready",
                PushScanReason::PlanSuperseded => "plan_superseded",
                PushScanReason::Question => "question",
                PushScanReason::Failed => "failed",
            }),
        ),
        PushPayload::Alert { .. } => ("alert", None),
        PushPayload::GardenDeleted { .. } => ("garden_deleted", None),
    }
}
