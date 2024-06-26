use std::sync::atomic;
use std::time::{SystemTime, UNIX_EPOCH};

/// Local time.
///
/// This clock is monotonic.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Ord, PartialOrd, Default)]
pub struct LocalTime {
    /// Milliseconds since Epoch.
    millis: u128,
}

impl std::fmt::Display for LocalTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_secs())
    }
}

impl LocalTime {
    /// Construct a local time from the current system time.
    pub fn now() -> Self {
        static LAST: atomic::AtomicU64 = atomic::AtomicU64::new(0);

        let now = Self::from(SystemTime::now()).as_secs();
        let last = LAST.load(atomic::Ordering::SeqCst);

        // If the current time is in the past, return the last recorded time instead.
        if now < last {
            Self::from_secs(last)
        } else {
            LAST.store(now, atomic::Ordering::SeqCst);
            LocalTime::from_secs(now)
        }
    }

    /// Construct a local time from whole seconds since Epoch.
    pub const fn from_secs(secs: u64) -> Self {
        Self {
            millis: secs as u128 * 1000,
        }
    }

    /// Construct a local time to whole seconds since Epoch.
    pub fn as_secs(&self) -> u64 {
        (self.millis / 1000).try_into().unwrap()
    }

    /// Get the duration since the given time.
    ///
    /// # Panics
    ///
    /// This function will panic if `earlier` is later than `self`.
    pub fn duration_since(&self, earlier: LocalTime) -> LocalDuration {
        LocalDuration::from_millis(
            self.millis
                .checked_sub(earlier.millis)
                .expect("supplied time is later than self"),
        )
    }

    /// Get the difference between two times.
    pub fn diff(&self, other: LocalTime) -> LocalDuration {
        if self > &other {
            self.duration_since(other)
        } else {
            other.duration_since(*self)
        }
    }

    /// Elapse time.
    ///
    /// Adds the given duration to the time.
    pub fn elapse(&mut self, duration: LocalDuration) {
        self.millis += duration.as_millis()
    }
}

/// Convert a `SystemTime` into a local time.
impl From<SystemTime> for LocalTime {
    fn from(system: SystemTime) -> Self {
        let millis = system.duration_since(UNIX_EPOCH).unwrap().as_millis();

        Self { millis }
    }
}

/// Substract two local times. Yields a duration.
impl std::ops::Sub<LocalTime> for LocalTime {
    type Output = LocalDuration;

    fn sub(self, other: LocalTime) -> LocalDuration {
        LocalDuration(self.millis.saturating_sub(other.millis))
    }
}

/// Substract a duration from a local time. Yields a local time.
impl std::ops::Sub<LocalDuration> for LocalTime {
    type Output = LocalTime;

    fn sub(self, other: LocalDuration) -> LocalTime {
        LocalTime {
            millis: self.millis - other.0,
        }
    }
}

/// Add a duration to a local time. Yields a local time.
impl std::ops::Add<LocalDuration> for LocalTime {
    type Output = LocalTime;

    fn add(self, other: LocalDuration) -> LocalTime {
        LocalTime {
            millis: self.millis + other.0,
        }
    }
}

/// Time duration as measured locally.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct LocalDuration(u128);

impl LocalDuration {
    /// The time interval between blocks. The "block time".
    pub const BLOCK_INTERVAL: LocalDuration = Self::from_mins(10);

    /// Maximum duration.
    pub const MAX: LocalDuration = LocalDuration(u128::MAX);

    /// Create a new duration from whole seconds.
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs as u128 * 1000)
    }

    /// Create a new duration from whole minutes.
    pub const fn from_mins(mins: u64) -> Self {
        Self::from_secs(mins * 60)
    }

    /// Construct a new duration from milliseconds.
    pub const fn from_millis(millis: u128) -> Self {
        Self(millis)
    }

    /// Return the number of minutes in this duration.
    pub const fn as_mins(&self) -> u64 {
        self.as_secs() / 60
    }

    /// Return the number of seconds in this duration.
    pub const fn as_secs(&self) -> u64 {
        (self.0 / 1000) as u64
    }

    /// Return the number of milliseconds in this duration.
    pub const fn as_millis(&self) -> u128 {
        self.0
    }
}

impl std::fmt::Display for LocalDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            dur if dur.as_millis() < 1000 => write!(f, "{} millisecond(s)", self.as_millis()),
            dur if dur.as_secs() < 60 => {
                let fraction = self.as_millis() % 1000;
                if fraction > 0 {
                    return write!(f, "{}.{} second(s)", self.as_secs(), fraction);
                }
                write!(f, "{} second(s)", self.as_secs())
            }
            dur if dur.as_mins() < 60 => {
                let fraction = self.as_secs() % 60;
                if fraction > 0 {
                    return write!(
                        f,
                        "{:.2} minutes(s)",
                        self.as_mins() as f64 + (fraction as f64 / 60.)
                    );
                }
                write!(f, "{} minutes(s)", self.as_mins())
            }
            _ => {
                let fraction = self.as_mins() % 60;
                if fraction > 0 {
                    return write!(f, "{:.2} hour(s)", self.as_mins() as f64 / 60.);
                }
                write!(f, "{} hour(s)", self.as_mins() / 60)
            }
        }
    }
}

impl<'a> std::iter::Sum<&'a LocalDuration> for LocalDuration {
    fn sum<I: Iterator<Item = &'a LocalDuration>>(iter: I) -> LocalDuration {
        let mut total: u128 = 0;

        for entry in iter {
            total = total
                .checked_add(entry.0)
                .expect("iter::sum should not overflow");
        }
        Self(total)
    }
}

impl std::ops::Add<LocalDuration> for LocalDuration {
    type Output = LocalDuration;

    fn add(self, other: LocalDuration) -> LocalDuration {
        LocalDuration(self.0 + other.0)
    }
}

impl std::ops::Div<u32> for LocalDuration {
    type Output = LocalDuration;

    fn div(self, other: u32) -> LocalDuration {
        LocalDuration(self.0 / other as u128)
    }
}

impl std::ops::Mul<u64> for LocalDuration {
    type Output = LocalDuration;

    fn mul(self, other: u64) -> LocalDuration {
        LocalDuration(self.0 * other as u128)
    }
}

impl From<LocalDuration> for std::time::Duration {
    fn from(other: LocalDuration) -> Self {
        std::time::Duration::from_millis(other.0 as u64)
    }
}

/// Manages timers and triggers timeouts.
pub struct TimeoutManager<K> {
    timeouts: Vec<(K, LocalTime)>,
    threshold: LocalDuration,
}

impl<K> TimeoutManager<K> {
    /// Create a new timeout manager.
    ///
    /// Takes a threshold below which two timeouts cannot overlap.
    pub fn new(threshold: LocalDuration) -> Self {
        Self {
            timeouts: vec![],
            threshold,
        }
    }

    /// Return the number of timeouts being tracked.
    pub fn len(&self) -> usize {
        self.timeouts.len()
    }

    /// Check whether there are timeouts being tracked.
    pub fn is_empty(&self) -> bool {
        self.timeouts.is_empty()
    }

    /// Register a new timeout with an associated key and wake-up time.
    pub fn register(&mut self, key: K, time: LocalTime) -> bool {
        // If this timeout is too close to a pre-existing timeout,
        // don't register it.
        if self
            .timeouts
            .iter()
            .any(|(_, t)| t.diff(time) < self.threshold)
        {
            return false;
        }

        self.timeouts.push((key, time));
        self.timeouts.sort_unstable_by(|(_, a), (_, b)| b.cmp(a));

        true
    }

    /// Get the minimum time duration we should wait for at least one timeout
    /// to be reached.  Returns `None` if there are no timeouts.
    pub fn next(&self, now: impl Into<LocalTime>) -> Option<LocalDuration> {
        let now = now.into();

        self.timeouts.last().map(|(_, t)| {
            if *t >= now {
                *t - now
            } else {
                LocalDuration::from_secs(0)
            }
        })
    }

    /// Given the current time, populate the input vector with the keys that
    /// have timed out. Returns the number of keys that timed out.
    pub fn wake(&mut self, now: LocalTime, woken: &mut Vec<K>) -> usize {
        let before = woken.len();

        while let Some((k, t)) = self.timeouts.pop() {
            if now >= t {
                woken.push(k);
            } else {
                self.timeouts.push((k, t));
                break;
            }
        }
        woken.len() - before
    }
}
