//! Rate limits ([architecture 11], abuse and cost): governor keyed limiters, in memory, which is
//! right for the single `api` instance ([architecture 12], operations). The daily scan limit and
//! the chat token budget are not rate limits: they are counters in the database
//! ([database 4], limits and counters), checked by the store and the agent, and answered with
//! `429 budget_exceeded`.

use std::num::NonZeroU32;

use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter, clock::Clock as _};

use domain::{id::UserId, rules::limits::INVITE_ATTEMPTS_PER_MINUTE};

use crate::error::ApiError;

/// The keyed limiters, keyed by the authenticated user, so an unauthenticated caller can't
/// fill the key map with strings of its choosing (the limiter runs after the token check).
pub struct RateLimits {
    /// Every authenticated request: `RATE_PER_MINUTE` (default 120, a placeholder: the documents
    /// give no number).
    pub per_user: DefaultKeyedRateLimiter<UserId>,
    /// `POST /invites/{code}/accept`: at most 10 in any minute per user ([contracts 9]), as a
    /// burst of 5 then one every 12 s. `Quota::per_minute(10)` would be a burst of 10 refilling one
    /// every 6 s, which lets about 20 through in a fresh minute; a burst of `n / 2` refilling
    /// `n / 2` a minute never lets more than `n` through in any 60 s.
    pub invite_accept: DefaultKeyedRateLimiter<UserId>,
}

impl RateLimits {
    /// Limiters with `per_minute` for the general limit.
    pub fn new(per_minute: u32) -> Self {
        let general = NonZeroU32::new(per_minute).unwrap_or(NonZeroU32::MIN);
        let invites = NonZeroU32::new(INVITE_ATTEMPTS_PER_MINUTE / 2).unwrap_or(NonZeroU32::MIN);
        Self {
            per_user: RateLimiter::keyed(Quota::per_minute(general)),
            invite_accept: RateLimiter::keyed(Quota::per_minute(invites)),
        }
    }

    /// Prunes keys whose cells have all replenished; run every minute by the scheduler, because
    /// a keyed limiter otherwise keeps every key it has seen.
    pub fn retain_recent(&self) {
        for limiter in [&self.per_user, &self.invite_accept] {
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    }
}

/// Checks one key: `429 rate_limited` with `retry_after_s` rounded up, never 0.
pub fn check(limiter: &DefaultKeyedRateLimiter<UserId>, user: UserId) -> Result<(), ApiError> {
    limiter.check_key(&user).map_err(|not_until| {
        let wait = not_until.wait_time_from(limiter.clock().now());
        let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
        ApiError::rate_limited(u32::try_from(seconds).unwrap_or(u32::MAX))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::error::ErrorCode;

    #[test]
    fn the_sixth_quick_invite_attempt_waits_12_seconds() {
        let limits = RateLimits::new(120);
        let user: UserId = "01JAX3K9T6R2B8V0QZ4M7D1E5W".parse().unwrap();
        for _ in 0..5 {
            assert!(check(&limits.invite_accept, user).is_ok());
        }
        let error = check(&limits.invite_accept, user).unwrap_err();
        assert_eq!(error.code(), ErrorCode::RateLimited);
        let body = serde_json::to_value(error.body("req_1")).unwrap();
        // 5 at once, then one every 12 s: at most 5 + 60 / 12 = 10 in any 60 s.
        assert_eq!(body["error"]["retry_after_s"], 12);
    }
}
