//! The scheduler ([architecture 8.3], [architecture 8.4]; [database 4], cleanup): background
//! tasks, each on its own interval, on the process `TaskTracker`, each ending when the shutdown
//! token fires. A failed run is logged and the next tick tries again; every task is idempotent
//! (alert keys, compare-and-set claims, batched deletes), so an overlap or a repeat is harmless.

pub mod triggers;

use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use tokio::time::MissedTickBehavior;

use domain::{
    garden::{gardens::SoilProfileStatus, push::PushPayload},
    id::GardenId,
    ports::{CellRequest, IdGenExt, NewTask, ObjectPrefix},
    records::{
        plan::TriggerKind,
        task::{AlertKey, TaskSource, TaskTrigger},
        weather::{CellWeather, WeatherCell},
    },
    rules::limits::{
        CONNECT_CLAIM_HOLD, DELETE_BATCH, FINISHED_JOB_KEEP, PENDING_CONNECT_TTL, TOMBSTONE_KEEP,
        WEATHER_CACHE_KEEP, soil_retry_after,
    },
    wire::{LocalDate, Timestamp, V1},
};

use blob::DeleteJob;

use crate::{error::ApiError, push::Pusher, state::AppState};

/// One scheduled task: its name (the log field), interval and body.
pub struct Task {
    /// For the logs.
    pub name: &'static str,
    /// Time between the start of two runs.
    pub every: Duration,
    /// Delay before the first run, so start-up isn't one burst of queries.
    pub first_after: Duration,
}

/// Every task of the table in api.md section 16. The 10-minute question continuation is part
/// of `sweep` (`Pipeline::sweep` owns it), not a task of its own.
pub const TASKS: [Task; 9] = [
    Task {
        name: "requeue",
        every: Duration::from_secs(5),
        first_after: Duration::from_secs(5),
    },
    Task {
        name: "sweep",
        every: Duration::from_secs(30),
        first_after: Duration::from_secs(10),
    },
    Task {
        name: "idle",
        every: Duration::from_mins(1),
        first_after: Duration::from_mins(1),
    },
    Task {
        name: "beliefs",
        every: Duration::from_secs(10),
        first_after: Duration::from_secs(20),
    },
    Task {
        name: "cleanup",
        every: Duration::from_mins(10),
        first_after: Duration::from_mins(1),
    },
    Task {
        name: "weather",
        every: Duration::from_hours(1),
        first_after: Duration::from_secs(30),
    },
    Task {
        name: "soil",
        every: Duration::from_mins(15),
        first_after: Duration::from_mins(2),
    },
    Task {
        name: "receipts",
        every: Duration::from_mins(15),
        first_after: Duration::from_mins(15),
    },
    Task {
        name: "orphans",
        every: Duration::from_hours(24),
        first_after: Duration::from_hours(1),
    },
];

/// Starts every task on the tracker.
///
/// The farm connectors' `connectors` task (roadmap E4.1) isn't here yet. When it comes, it
/// starts only when [`AppState::connector_key`] is `Some`, so with connectors off (no
/// `CONNECTOR_KEY`, decision 9) it never runs, and it takes that key to unseal the tokens.
pub fn start(state: &Arc<AppState>) {
    for task in &TASKS {
        match task.name {
            "requeue" => every(state, task, requeue),
            "sweep" => every(state, task, sweep),
            "idle" => every(state, task, idle),
            "beliefs" => every(state, task, beliefs),
            "cleanup" => every(state, task, cleanup),
            "weather" => every(state, task, weather_and_triggers),
            "soil" => every(state, task, soil),
            "receipts" => every(state, task, receipts),
            _ => every(state, task, orphans),
        }
    }
}

/// Runs `body` every `task.every` until shutdown. `F` is a plain function from the state to a
/// future; both are `Send + 'static` because the loop is spawned and owns them.
fn every<F, Fut>(state: &Arc<AppState>, task: &Task, body: F)
where
    F: Fn(Arc<AppState>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), ApiError>> + Send + 'static,
{
    let state = Arc::clone(state);
    let (name, period, first) = (task.name, task.every, task.first_after);
    let shutdown = state.shutdown.clone();
    state.tasks.clone().spawn(async move {
        let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + first, period);
        // A run that overran doesn't cause a burst of catch-up runs.
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => {}
            }
            if let Err(error) = body(Arc::clone(&state)).await {
                tracing::warn!(task = name, error = %error, "scheduled task failed");
            }
        }
    });
}

/// Requeues expired leases and the jobs of workers silent for 30 s; a job out of attempts fails
/// for good through the pipeline ([contracts 5]). The store wakes the claims for the requeued
/// jobs after its commit.
async fn requeue(state: Arc<AppState>) -> Result<(), ApiError> {
    let requeued = state
        .ports
        .store
        .requeue_stale(state.ports.clock.now())
        .await?;
    for job in requeued.into_iter().filter(|r| r.exhausted) {
        let follow_up = state.pipeline.job_failed_for_good(job.job).await?;
        state.spawn_follow_up(follow_up);
    }
    Ok(())
}

/// `Pipeline::sweep`, the one owner of recovery and continuation: stale steps (2 minutes; 5 for
/// `plan`) and question photos stuck in `measuring` for 2 minutes, re-claimed by
/// compare-and-set and re-run ([contracts 3], the sweep), and questions unanswered for 10
/// minutes, continued with the best candidate marked uncertain.
async fn sweep(state: Arc<AppState>) -> Result<(), ApiError> {
    for follow_up in state.pipeline.sweep().await? {
        state.spawn_follow_up(Some(follow_up));
    }
    Ok(())
}

/// In-memory upkeep: limiter keys, idle broadcast channels, unheld plan locks.
async fn idle(state: Arc<AppState>) -> Result<(), ApiError> {
    state.limits.retain_recent();
    state.hub.sweep_idle();
    state.writers.sweep_idle();
    Ok(())
}

/// Shared memories per run of the belief sweep.
const BELIEF_SYNC_BATCH: u32 = 50;

/// The belief sweep, the only writer to Agent Memory (architecture 7, memory): erases the
/// queued subjects, then records the unsynced shared memories. Every 10 s, so a new shared
/// memory reaches Agent Memory within seconds and a deleted one is erased as fast. With Agent
/// Memory off it only empties the erase queue.
async fn beliefs(state: Arc<AppState>) -> Result<(), ApiError> {
    let p = &state.ports;
    let report = agent::beliefs::sync_pending(
        p.memory.as_ref(),
        p.beliefs.as_ref(),
        p.clock.as_ref(),
        BELIEF_SYNC_BATCH,
    )
    .await?;
    if report != agent::beliefs::SweepReport::default() {
        tracing::info!(
            forgotten = report.forgotten,
            recorded = report.recorded,
            failed = report.failed,
            "belief sweep"
        );
    }
    Ok(())
}

/// No TTL in `SurrealDB` ([database 4], cleanup): expired idempotency keys, weather older than
/// 2 hours, expired pending actions, finished jobs older than 7 days, account tombstones a day
/// after the account was confirmed gone at Clerk, and farm connectors still `pending` 10 minutes
/// after their connect started ([connectors 5]).
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
async fn cleanup(state: Arc<AppState>) -> Result<(), ApiError> {
    let now = state.ports.clock.now();
    let store = &state.ports.store;
    store.delete_expired(now).await?;
    store.delete_before(now.minus(WEATHER_CACHE_KEEP)).await?;
    store.expire_pending_actions(now).await?;
    store
        .delete_finished_jobs(now.minus(FINISHED_JOB_KEEP))
        .await?;
    store
        .delete_finished_tombstones(now.minus(TOMBSTONE_KEEP))
        .await?;
    delete_expired_connects(&state, now).await?;
    Ok(())
}

/// Deletes the pending connectors whose connect started more than [`PENDING_CONNECT_TTL`]
/// before `now`, a batch of up to 500 at a time until a batch comes back short, except those
/// a `complete` claimed less than [`CONNECT_CLAIM_HOLD`] ago, whose exchange may still be
/// running. A pending connector holds no tokens and no records, so nothing is revoked.
pub async fn delete_expired_connects(state: &AppState, now: Timestamp) -> Result<(), ApiError> {
    let cutoff = now.minus(PENDING_CONNECT_TTL);
    let claimed_cutoff = now.minus(CONNECT_CLAIM_HOLD);
    loop {
        let deleted = state
            .ports
            .store
            .delete_expired_pending(cutoff, claimed_cutoff)
            .await?;
        if deleted > 0 {
            tracing::info!(deleted, "expired pending connectors deleted");
        }
        if deleted < DELETE_BATCH {
            return Ok(());
        }
    }
}

/// The hour a weather cache row belongs to.
pub fn hour_of(now: Timestamp) -> Timestamp {
    let secs = now.unix_seconds();
    chrono::DateTime::from_timestamp(secs - secs.rem_euclid(3600), 0).map_or(now, Timestamp::from)
}

/// Fetches the cells missing from the cache and stores them, adding each to `weather`.
async fn fetch_missing_weather(
    state: &AppState,
    missing: &[CellRequest],
    hour: Timestamp,
    weather: &mut HashMap<WeatherCell, CellWeather>,
) -> Result<(), ApiError> {
    // Batches of at most 50 cells, a failed batch retried once, then cell by cell; a cell that
    // still fails skips its gardens until the next run (contracts 7, fetching and caching).
    for result in integrations::open_meteo::fetch_cells(&*state.ports.weather, missing).await {
        match result.weather {
            Ok(fetched) => {
                state
                    .ports
                    .store
                    .put(result.cell, hour, fetched.clone())
                    .await?;
                weather.insert(result.cell, fetched);
            }
            Err(error) => {
                tracing::warn!(cell = %result.cell, error = %error, "weather cell skipped");
            }
        }
    }
    Ok(())
}

/// Creates a task for each active plan trigger rule that fired today in its garden.
async fn create_plan_trigger_tasks(
    state: &AppState,
    fired_by_garden: &HashMap<GardenId, Vec<(TriggerKind, LocalDate)>>,
    now: Timestamp,
) -> Result<(), ApiError> {
    let gardens: Vec<GardenId> = fired_by_garden.keys().copied().collect();
    for rule in state.ports.store.active_triggers(&gardens).await? {
        let Some(fired) = fired_by_garden.get(&rule.garden) else {
            continue;
        };
        for &(trigger, date) in fired.iter().filter(|(t, _)| *t == rule.trigger) {
            if rule.until.is_some_and(|until| date > until) {
                continue;
            }
            let task = NewTask {
                id: state.ports.ids.id(),
                plant: Some(rule.plant),
                plan: Some(rule.plan),
                plan_step: Some(rule.step.clone()),
                scan: None,
                title: rule.title.clone(),
                detail: Some(rule.detail.clone()),
                due: Some(date),
                repeat: None,
                source: TaskSource::Plan,
                trigger: Some(TaskTrigger {
                    kind: trigger,
                    date,
                }),
                alert_key: Some(AlertKey::PlanTrigger {
                    plan: rule.plan,
                    step: rule.step.clone(),
                    date,
                }),
                created_by: None,
                created_at: now,
            };
            // A dead plant or a duplicate key creates nothing (contracts 7, tasks only on live
            // plants); the store answers `None`.
            state
                .ports
                .store
                .create_alert_task(rule.garden, task)
                .await?;
        }
    }
    Ok(())
}

/// Weather by cell, forecast alerts and plan triggers ([architecture 8.3]): group the gardens by
/// cell, fetch each missing cell once in requests of up to 50 (a failed batch is retried once,
/// then cell by cell), cache it, then fire the rules. Alert tasks are deduplicated by their
/// unique `alert_key`, so overlapping runs create each alert once.
async fn weather_and_triggers(state: Arc<AppState>) -> Result<(), ApiError> {
    let now = state.ports.clock.now();
    let hour = hour_of(now);
    let cells = state.ports.store.gardens_by_cell().await?;
    let mut weather: HashMap<WeatherCell, CellWeather> = HashMap::new();
    let mut missing = Vec::new();
    for cell in &cells {
        match state.ports.store.cached(cell.cell, hour).await? {
            Some(w) => {
                weather.insert(cell.cell, w);
            }
            None => missing.push(CellRequest {
                cell: cell.cell,
                time_zone: cell.time_zone.clone(),
            }),
        }
    }
    fetch_missing_weather(&state, &missing, hour, &mut weather).await?;
    let thresholds = state.config.weather;
    let pusher = Pusher::new(state.ports.store.clone(), state.ports.notifier.clone());
    let mut fired_by_garden: HashMap<GardenId, Vec<_>> = HashMap::new();
    for cell in &cells {
        let Some(w) = weather.get(&cell.cell) else {
            continue;
        };
        let fired = triggers::fired(w, &thresholds);
        for &garden in &cell.gardens {
            fired_by_garden.insert(garden, fired.clone());
            for &(trigger, date) in &fired {
                let (title, detail) = triggers::alert_text(trigger);
                let task = NewTask {
                    id: state.ports.ids.id(),
                    plant: None,
                    plan: None,
                    plan_step: None,
                    scan: None,
                    title: title.to_owned(),
                    detail: Some(detail.to_owned()),
                    due: Some(date),
                    repeat: None,
                    source: TaskSource::ForecastAlert,
                    trigger: Some(TaskTrigger {
                        kind: trigger,
                        date,
                    }),
                    alert_key: Some(AlertKey::Forecast {
                        garden,
                        trigger,
                        date,
                    }),
                    created_by: None,
                    created_at: now,
                };
                if let Some(created) = state.ports.store.create_alert_task(garden, task).await? {
                    let members = pusher.members(garden).await;
                    let payload = PushPayload::Alert {
                        v: V1,
                        garden_id: garden,
                        task_id: created.id,
                        trigger,
                    };
                    pusher.send(&members, title, detail, payload).await;
                }
            }
        }
    }
    create_plan_trigger_tasks(&state, &fired_by_garden, now).await?;
    Ok(())
}

/// `SoilGrids` retries ([contracts 9], the soil profile): at most 4 due gardens a run, inside
/// its 5 calls a minute ([architecture 6.4]). A garden is due when `pending`, or `unavailable`
/// past its `soil_retry_at`: 15 minutes after an outage, 7 days after no data. A null-pixel
/// garden costs its 9 paced calls (the point and 8 probes) once a week, not every 15 minutes;
/// an outage stops at the first call.
async fn soil(state: Arc<AppState>) -> Result<(), ApiError> {
    let now = state.ports.clock.now();
    for (garden, at) in state.ports.store.gardens_due_soil_profile(now, 4).await? {
        fetch_soil_profile(&state, garden, at).await?;
    }
    Ok(())
}

/// One `SoilGrids` attempt for a garden, from `POST /gardens` or the `soil` task: the profile,
/// or `unavailable` with the next attempt `soil_retry_after` the failure.
pub async fn fetch_soil_profile(
    state: &AppState,
    garden: GardenId,
    at: domain::records::weather::Coordinates,
) -> Result<(), ApiError> {
    match state.ports.soil.soil_profile(at).await {
        Ok(profile) => {
            state
                .ports
                .store
                .set_soil_profile(garden, SoilProfileStatus::Ready, Some(profile))
                .await?;
        }
        Err(error) => {
            let wait = soil_retry_after(&error);
            tracing::info!(%garden, error = %error, retry_in_s = wait.as_secs(), "SoilGrids failed; retried later");
            let retry_at = state.ports.clock.now().plus(wait);
            state
                .ports
                .store
                .soil_profile_failed(garden, retry_at)
                .await?;
        }
    }
    Ok(())
}

/// Expo push receipts, 15 minutes after sending: forget `DeviceNotRegistered` tokens.
async fn receipts(state: Arc<AppState>) -> Result<(), ApiError> {
    let dead = state.ports.notifier.check_receipts().await?;
    if !dead.is_empty() {
        state.ports.store.forget_push_tokens(&dead).await?;
    }
    Ok(())
}

/// The daily orphan sweep ([storage 4], uploads): objects older than an hour under a garden's
/// prefix that no record points to. Deletes go through the `DeleteQueue`: detached, never
/// cancelled, retried, drained at shutdown.
async fn orphans(state: Arc<AppState>) -> Result<(), ApiError> {
    let cutoff = state.ports.clock.now().minus(Duration::from_hours(1));
    for cell in state.ports.store.gardens_by_cell().await? {
        for garden in cell.gardens {
            let old: Vec<_> = state
                .ports
                .blob
                .list(&ObjectPrefix::garden(garden))
                .await?
                .into_iter()
                .filter(|o| o.last_modified < cutoff)
                .map(|o| o.key)
                .collect();
            if old.is_empty() {
                continue;
            }
            let orphans = state.ports.store.unreferenced_keys(&old).await?;
            if orphans.is_empty() {
                continue;
            }
            let outcome = state
                .deletes
                .submit(DeleteJob::Keys(orphans))
                .outcome()
                .await;
            if !outcome.is_some_and(|o| o.is_done()) {
                tracing::warn!(%garden, "orphans not deleted; the next sweep retries");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_truncate() {
        let t: Timestamp = "2026-10-18T14:37:12Z".parse().unwrap();
        assert_eq!(hour_of(t).to_string(), "2026-10-18T14:00:00Z");
    }
}
