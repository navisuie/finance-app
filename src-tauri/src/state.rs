use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use finance_core::settings::DEFAULT_AUTO_LOCK_MINUTES;
use finance_core::throttle::Throttle;
use finance_core::{CoreError, Result, Session};

/// Everything the backend keeps in memory. The session (and with it the
/// database key) exists only while the vault is unlocked.
pub struct AppState {
    pub data_dir: PathBuf,
    inner: Mutex<Inner>,
    pub throttle: Mutex<Throttle>,
}

struct Inner {
    session: Option<Session>,
    last_activity: Instant,
    auto_lock: Duration,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        AppState {
            data_dir,
            inner: Mutex::new(Inner {
                session: None,
                last_activity: Instant::now(),
                auto_lock: Duration::from_secs(DEFAULT_AUTO_LOCK_MINUTES as u64 * 60),
            }),
            throttle: Mutex::new(Throttle::default()),
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned mutex means a panic happened mid-operation; recover the
        // data rather than crash, but drop any open session to be safe.
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                let mut g = poisoned.into_inner();
                g.session = None;
                g
            }
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.lock_inner().session.is_some()
    }

    pub fn set_session(&self, session: Session, auto_lock_minutes: u32) {
        let mut g = self.lock_inner();
        g.session = Some(session);
        g.last_activity = Instant::now();
        g.auto_lock = Duration::from_secs(auto_lock_minutes as u64 * 60);
    }

    pub fn set_auto_lock(&self, minutes: u32) {
        self.lock_inner().auto_lock = Duration::from_secs(minutes as u64 * 60);
    }

    /// Lock now. Returns true if a session was open.
    pub fn lock(&self) -> bool {
        self.lock_inner().session.take().is_some()
    }

    /// Record user activity (called by the UI, throttled on its side).
    pub fn touch(&self) -> Result<()> {
        let mut g = self.lock_inner();
        Self::expire_if_idle(&mut g);
        if g.session.is_none() {
            return Err(CoreError::Locked);
        }
        g.last_activity = Instant::now();
        Ok(())
    }

    /// Lock if idle for longer than the auto-lock period. Returns true if this
    /// call locked the vault.
    pub fn enforce_idle_timeout(&self) -> bool {
        let mut g = self.lock_inner();
        Self::expire_if_idle(&mut g)
    }

    fn expire_if_idle(g: &mut Inner) -> bool {
        if g.session.is_some() && g.last_activity.elapsed() >= g.auto_lock {
            g.session = None;
            return true;
        }
        false
    }

    /// Run `f` with the open session. Enforces the idle timeout first, so a
    /// request that arrives after the timeout is refused even if the
    /// background timer has not fired yet. Counts as activity.
    pub fn with_session<T>(&self, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        let mut g = self.lock_inner();
        Self::expire_if_idle(&mut g);
        g.last_activity = Instant::now();
        match g.session.as_mut() {
            Some(s) => f(s),
            None => Err(CoreError::Locked),
        }
    }
}
