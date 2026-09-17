//! Slows down repeated unlock attempts. The KDF already costs ~1 s per guess;
//! this adds an escalating lockout after a few failures.

use std::time::{Duration, Instant};

use crate::error::{CoreError, Result};

const FREE_ATTEMPTS: u32 = 3;
const MAX_DELAY_SECS: u64 = 300;

#[derive(Debug, Default)]
pub struct Throttle {
    failures: u32,
    blocked_until: Option<Instant>,
}

impl Throttle {
    pub fn check(&self) -> Result<()> {
        self.check_at(Instant::now())
    }

    fn check_at(&self, now: Instant) -> Result<()> {
        if let Some(until) = self.blocked_until {
            if now < until {
                let secs = (until - now).as_secs().max(1);
                return Err(CoreError::TooManyAttempts { retry_after_secs: secs });
            }
        }
        Ok(())
    }

    pub fn record_failure(&mut self) {
        self.record_failure_at(Instant::now())
    }

    fn record_failure_at(&mut self, now: Instant) {
        self.failures += 1;
        if self.failures >= FREE_ATTEMPTS {
            let exp = (self.failures - FREE_ATTEMPTS).min(16);
            let secs = (5u64 << exp).min(MAX_DELAY_SECS);
            self.blocked_until = Some(now + Duration::from_secs(secs));
        }
    }

    pub fn record_success(&mut self) {
        *self = Throttle::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalates_after_free_attempts() {
        let mut t = Throttle::default();
        let now = Instant::now();
        t.record_failure_at(now);
        t.record_failure_at(now);
        assert!(t.check_at(now).is_ok());
        t.record_failure_at(now); // 3rd failure -> 5 s
        assert!(t.check_at(now).is_err());
        assert!(t.check_at(now + Duration::from_secs(6)).is_ok());
        t.record_failure_at(now); // 4th -> 10 s
        match t.check_at(now + Duration::from_secs(1)) {
            Err(CoreError::TooManyAttempts { retry_after_secs }) => assert_eq!(retry_after_secs, 9),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn caps_delay_and_resets_on_success() {
        let mut t = Throttle::default();
        let now = Instant::now();
        for _ in 0..40 {
            t.record_failure_at(now);
        }
        assert!(t.check_at(now + Duration::from_secs(MAX_DELAY_SECS + 1)).is_ok());
        t.record_success();
        assert!(t.check_at(now).is_ok());
    }
}
