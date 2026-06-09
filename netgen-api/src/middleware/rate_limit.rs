use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use uuid::Uuid;

use super::UserTier;

// ─── String-keyed rate limiter (auth endpoints + IP-based limiting) ───────────

/// Rate limiter keyed by an arbitrary string — email addresses or IP addresses.
pub struct AuthRateLimiter {
    window: Duration,
    max: usize,
    state: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AuthRateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self { window, max, state: Mutex::new(HashMap::new()) }
    }

    /// 10 attempts per 15 minutes — suitable for login rate limiting.
    pub fn login() -> Self {
        Self::new(10, Duration::from_secs(900))
    }

    /// 5 registrations per hour — suitable for register rate limiting.
    pub fn register() -> Self {
        Self::new(5, Duration::from_secs(3600))
    }

    /// 20 requests per minute — suitable for unauthenticated API calls.
    pub fn unauthenticated() -> Self {
        Self::new(20, Duration::from_secs(60))
    }

    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut state = self.state.lock().expect("auth rate limiter lock poisoned");
        let calls = state.entry(key.to_string()).or_default();
        calls.retain(|&ts| now.duration_since(ts) < self.window);
        if calls.len() < self.max {
            calls.push(now);
            Ok(())
        } else {
            let oldest = calls[0];
            let elapsed = now.duration_since(oldest);
            let retry_after = self.window.saturating_sub(elapsed).as_secs() + 1;
            Err(retry_after)
        }
    }
}

// ─── Sliding window rate limiter ─────────────────────────────────────────────

/// Per-user sliding window rate limiter.
///
/// Stores the timestamps of each call within the window. On each call, expired
/// entries are pruned first, then the count is checked against the tier limit.
/// Enterprise tier (limit=0) always passes.
pub struct RateLimiter {
    window: Duration,
    state: Mutex<HashMap<Uuid, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// 1-hour sliding window — the default for AI calls.
    pub fn hourly() -> Self {
        Self::new(Duration::from_secs(3600))
    }

    /// Check whether `user_id` may make another call, given their tier.
    ///
    /// Returns `Ok(())` if allowed, `Err(retry_after_secs)` if rate-limited.
    pub fn check(&self, user_id: Uuid, tier: &UserTier) -> Result<(), u64> {
        let limit = tier.ai_calls_per_hour();
        if limit == 0 {
            return Ok(()); // Enterprise — unlimited
        }

        let now = Instant::now();
        let mut state = self.state.lock().expect("rate limiter lock poisoned");
        let calls = state.entry(user_id).or_default();

        // Prune expired timestamps
        calls.retain(|&ts| now.duration_since(ts) < self.window);

        if calls.len() < limit {
            calls.push(now);
            Ok(())
        } else {
            // Earliest call in the window — wait until it expires
            let oldest = calls[0];
            let elapsed = now.duration_since(oldest);
            let retry_after = self.window.saturating_sub(elapsed).as_secs() + 1;
            Err(retry_after)
        }
    }

    /// Current call count for a user within the active window (for diagnostics).
    pub fn current_count(&self, user_id: Uuid) -> usize {
        let now = Instant::now();
        let mut state = self.state.lock().expect("rate limiter lock poisoned");
        let calls = state.entry(user_id).or_default();
        calls.retain(|&ts| now.duration_since(ts) < self.window);
        calls.len()
    }

    /// Remove all state for a user (e.g., on logout or account deletion).
    pub fn clear_user(&self, user_id: Uuid) {
        let mut state = self.state.lock().expect("rate limiter lock poisoned");
        state.remove(&user_id);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid {
        Uuid::new_v4()
    }

    #[test]
    fn free_tier_allows_up_to_limit() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..5 {
            assert!(rl.check(id, &UserTier::Free).is_ok());
        }
    }

    #[test]
    fn free_tier_blocks_over_limit() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..5 {
            rl.check(id, &UserTier::Free).unwrap();
        }
        let result = rl.check(id, &UserTier::Free);
        assert!(result.is_err());
    }

    #[test]
    fn pro_tier_allows_up_to_100() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..100 {
            assert!(rl.check(id, &UserTier::Pro).is_ok());
        }
        assert!(rl.check(id, &UserTier::Pro).is_err());
    }

    #[test]
    fn enterprise_tier_never_limited() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..1000 {
            assert!(rl.check(id, &UserTier::Enterprise).is_ok());
        }
    }

    #[test]
    fn different_users_are_independent() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let a = uid();
        let b = uid();
        for _ in 0..5 {
            rl.check(a, &UserTier::Free).unwrap();
        }
        // b has fresh window — should still be allowed
        assert!(rl.check(b, &UserTier::Free).is_ok());
        // a is exhausted
        assert!(rl.check(a, &UserTier::Free).is_err());
    }

    #[test]
    fn short_window_expires_and_allows_again() {
        let rl = RateLimiter::new(Duration::from_millis(50));
        let id = uid();
        for _ in 0..5 {
            rl.check(id, &UserTier::Free).unwrap();
        }
        assert!(rl.check(id, &UserTier::Free).is_err());

        std::thread::sleep(Duration::from_millis(60));

        // Window expired — all 5 slots are free again
        assert!(rl.check(id, &UserTier::Free).is_ok());
    }

    #[test]
    fn retry_after_is_nonzero_when_limited() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..5 {
            rl.check(id, &UserTier::Free).unwrap();
        }
        let err = rl.check(id, &UserTier::Free).unwrap_err();
        assert!(err > 0);
    }

    #[test]
    fn clear_user_resets_window() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        for _ in 0..5 {
            rl.check(id, &UserTier::Free).unwrap();
        }
        assert!(rl.check(id, &UserTier::Free).is_err());
        rl.clear_user(id);
        assert!(rl.check(id, &UserTier::Free).is_ok());
    }

    #[test]
    fn current_count_reflects_calls_made() {
        let rl = RateLimiter::new(Duration::from_secs(3600));
        let id = uid();
        assert_eq!(rl.current_count(id), 0);
        rl.check(id, &UserTier::Pro).unwrap();
        rl.check(id, &UserTier::Pro).unwrap();
        assert_eq!(rl.current_count(id), 2);
    }
}
