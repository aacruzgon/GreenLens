//! Fixed limits from the contracts, in one place (open questions 5 and 7 of contracts 12 may
//! move some of them).

use std::time::Duration;

use crate::{error::DomainError, ports::ProviderError, wire::Timestamp};

/// Daily scan limit per user, counted on `POST /scans` ([contracts 3]).
pub const DAILY_SCAN_LIMIT: u32 = 30;
/// Pending actions expire after 24 hours ([contracts 8]).
pub const PENDING_ACTION_TTL: Duration = Duration::from_hours(24);
/// Invites expire after 7 days ([contracts 9]).
pub const INVITE_TTL: Duration = Duration::from_hours(168);
/// Idempotency keys are kept for 24 hours ([contracts 1]).
pub const IDEMPOTENCY_TTL: Duration = Duration::from_hours(24);
/// Invite-code attempts per user per minute ([contracts 9]).
pub const INVITE_ATTEMPTS_PER_MINUTE: u32 = 10;
/// A chat message's text, in characters ([contracts 8]).
pub const MESSAGE_MAX_CHARS: usize = 2000;
/// Images per chat message ([contracts 8]).
pub const MESSAGE_MAX_IMAGES: usize = 3;
/// Weather cache rows older than this are deleted ([database 4], cleanup).
pub const WEATHER_CACHE_KEEP: Duration = Duration::from_hours(2);
/// Finished jobs older than this are deleted.
pub const FINISHED_JOB_KEEP: Duration = Duration::from_hours(168);
/// Cleanup and deletion batches ([database 4]).
pub const DELETE_BATCH: u32 = 500;
/// Open-Meteo multi-location requests carry at most 50 cells ([contracts 7]).
pub const WEATHER_CELLS_PER_REQUEST: usize = 50;
/// Up to 5 neighbours are given to the plan writer ([contracts 7]).
pub const MAX_PLAN_NEIGHBOURS: usize = 5;
/// Neighbours are pinned within 1.5 m, centre to centre ([contracts 9]).
pub const NEIGHBOUR_RADIUS_M: f64 = 1.5;
/// A plant is crowded when its nearest pin is closer than 0.75 × its spacing.
pub const CROWDING_FACTOR: f64 = 0.75;
/// At most 9 candidates in the pre-VLM set and in a diagnosis.
pub const MAX_CANDIDATES: usize = 9;
/// Every VLM call gets at most 4 images.
pub const MAX_VLM_IMAGES: usize = 4;
/// SSE keepalive comment interval ([contracts 4]).
pub const SSE_KEEPALIVE: Duration = Duration::from_secs(15);
/// An account's tombstone is kept this long after the auth provider confirmed its deletion:
/// far longer than a Clerk session token lives (60 seconds by default), so no token issued
/// before the deletion can outlive it.
pub const TOMBSTONE_KEEP: Duration = Duration::from_hours(24);
/// `SoilGrids` retry after a provider outage ([contracts 9], the soil profile).
pub const SOIL_RETRY_OUTAGE: Duration = Duration::from_mins(15);
/// `SoilGrids` retry after no data at the point and every nearby probe: a null pixel doesn't
/// change on a quick retry.
pub const SOIL_RETRY_NO_DATA: Duration = Duration::from_hours(168);

/// How long to wait before the next `SoilGrids` attempt after this failure: 7 days after
/// `NotFound` (null pixels), 15 minutes after anything else (timeout, connection error, `5xx`,
/// `429`, an unreadable answer). The status is `unavailable` either way.
pub const fn soil_retry_after(error: &ProviderError) -> Duration {
    match error {
        ProviderError::NotFound { .. } => SOIL_RETRY_NO_DATA,
        _ => SOIL_RETRY_OUTAGE,
    }
}

/// Checks a scan count after it was incremented in the scan's transaction ([database 4],
/// limits): over 30 is `429 budget_exceeded` until the next UTC day.
pub fn check_daily_scans(count_after: u32, now: Timestamp) -> Result<(), DomainError> {
    if count_after > DAILY_SCAN_LIMIT {
        Err(DomainError::BudgetExceeded {
            budget: crate::error::Budget::Scans,
            resets_at: next_utc_midnight(now),
        })
    } else {
        Ok(())
    }
}

/// The start of the next UTC day, when daily budgets reset.
pub fn next_utc_midnight(now: Timestamp) -> Timestamp {
    now.utc_day()
        .succ_opt()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or(now, |t| Timestamp::from(t.and_utc()))
}

/// Checks a chat message's limits.
pub fn check_message(text: &str, images: usize) -> Result<(), DomainError> {
    let chars = text.chars().count();
    if chars == 0 || chars > MESSAGE_MAX_CHARS {
        return Err(DomainError::invalid("text", "1-2000 characters"));
    }
    if images > MESSAGE_MAX_IMAGES {
        return Err(DomainError::invalid("image_ids", "at most 3 images"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soil_retries_wait_a_week_after_no_data_and_15_minutes_after_an_outage() {
        let no_data = ProviderError::NotFound {
            provider: "soilgrids",
        };
        let outage = ProviderError::Unavailable {
            provider: "soilgrids",
            source: "timeout".into(),
        };
        let limited = ProviderError::RateLimited {
            provider: "soilgrids",
            retry_after_s: Some(60),
        };
        assert_eq!(soil_retry_after(&no_data), Duration::from_hours(168));
        assert_eq!(soil_retry_after(&outage), Duration::from_mins(15));
        assert_eq!(soil_retry_after(&limited), Duration::from_mins(15));
    }

    #[test]
    fn daily_scans() {
        let now: Timestamp = "2026-10-18T14:03:22Z".parse().unwrap();
        assert!(check_daily_scans(30, now).is_ok());
        let err = check_daily_scans(31, now).unwrap_err();
        let DomainError::BudgetExceeded { resets_at, .. } = err else {
            panic!("expected budget_exceeded")
        };
        assert_eq!(resets_at.to_string(), "2026-10-19T00:00:00Z");
    }

    #[test]
    fn messages() {
        assert!(check_message("hello", 0).is_ok());
        assert!(check_message("", 0).is_err());
        assert!(check_message(&"a".repeat(2001), 0).is_err());
        assert!(check_message("hi", 4).is_err());
    }
}
