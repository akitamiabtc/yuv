//! Block time and other time-related types.
use crate::net::LocalTime;
use std::sync::{Arc, Mutex};
use std::{collections::HashSet, hash::Hash};

/// Maximum time adjustment between network and local time (70 minutes).
pub const MAX_TIME_ADJUSTMENT: TimeOffset = 70 * 60;

/// Minimum number of samples before we adjust local time.
pub const MIN_TIME_SAMPLES: usize = 5;

/// Maximum number of samples stored.
pub const MAX_TIME_SAMPLES: usize = 200;

/// A time offset, in seconds.
pub type TimeOffset = i64;

/// Clock that tells the time.
pub trait Clock: Clone {
    /// Tell the time in local time.
    fn local_time(&self) -> LocalTime;
}

/// A network-adjusted clock.
pub trait AdjustedClock<K>: Clock {
    /// Record a peer offset.
    fn record_offset(&mut self, source: K, sample: TimeOffset);
    /// Set the local time.
    fn set(&mut self, local_time: LocalTime);
}

impl<K: Eq + Clone + Hash> AdjustedClock<K> for AdjustedTime<K> {
    fn record_offset(&mut self, source: K, sample: TimeOffset) {
        AdjustedTime::record_offset(self, source, sample)
    }

    fn set(&mut self, local_time: LocalTime) {
        AdjustedTime::set_local_time(self, local_time)
    }
}

/// Clock with interior mutability.
#[derive(Debug, Clone)]
pub struct RefClock<T: Clock> {
    inner: Arc<Mutex<T>>,
}

impl<T: Clock> std::ops::Deref for RefClock<T> {
    type Target = Arc<Mutex<T>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K: Eq + Clone + Hash> AdjustedClock<K> for RefClock<AdjustedTime<K>> {
    fn record_offset(&mut self, source: K, sample: TimeOffset) {
        self.inner.lock().unwrap().record_offset(source, sample);
    }

    fn set(&mut self, local_time: LocalTime) {
        self.inner.lock().unwrap().set_local_time(local_time);
    }
}

impl<T: Clock> From<T> for RefClock<T> {
    fn from(other: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(other)),
        }
    }
}

impl<T: Clock> Clock for RefClock<T> {
    fn local_time(&self) -> LocalTime {
        self.inner.lock().unwrap().local_time()
    }
}

impl Clock for LocalTime {
    fn local_time(&self) -> LocalTime {
        *self
    }
}

/// Network-adjusted time tracker.
///
/// *Network-adjusted time* is the median timestamp of all connected peers.
/// Since we store only time offsets for each peer, the network-adjusted time is
/// the local time plus the median offset of all connected peers.
///
/// Nb. Network time is never adjusted more than 70 minutes from local system time.
#[derive(Debug, Clone)]
pub struct AdjustedTime<K> {
    /// Sample sources. Prevents us from getting two samples from the same source.
    sources: HashSet<K>,
    /// Time offset samples.
    samples: Vec<TimeOffset>,
    /// Current time offset, based on our samples.
    offset: TimeOffset,
    /// Last known local time.
    local_time: LocalTime,
}

impl<K: Eq + Clone + Hash> Clock for AdjustedTime<K> {
    fn local_time(&self) -> LocalTime {
        self.local_time()
    }
}

impl<K: Hash + Eq> Default for AdjustedTime<K> {
    fn default() -> Self {
        Self::new(LocalTime::default())
    }
}

impl<K: Hash + Eq> AdjustedTime<K> {
    /// Create a new network-adjusted time tracker.
    /// Starts with a single sample of zero.
    pub fn new(local_time: LocalTime) -> Self {
        let offset = 0;

        let mut samples = Vec::with_capacity(MAX_TIME_SAMPLES);
        samples.push(offset);

        let sources = HashSet::with_capacity(MAX_TIME_SAMPLES);

        Self {
            sources,
            samples,
            offset,
            local_time,
        }
    }

    /// Add a time sample to influence the network-adjusted time.
    pub fn record_offset(&mut self, source: K, sample: TimeOffset) {
        // Nb. This behavior is based on Bitcoin Core. An alternative is to truncate the
        // samples list, to never exceed `MAX_TIME_SAMPLES`, and allow new samples to be
        // added to the list, while the set of sample sources keeps growing. This has the
        // advantage that as new peers are discovered, the network time can keep adjusting,
        // while old samples get discarded. Such behavior is found in `btcd`.
        //
        // Another quirk of this implementation is that the actual number of samples can
        // reach `MAX_TIME_SAMPLES + 1`, since there is always an initial `0` sample with
        // no associated source.
        //
        // Finally, we never remove sources. Even after peers disconnect. This is congruent
        // with Bitcoin Core behavior. I'm not sure why that is.
        if self.sources.len() == MAX_TIME_SAMPLES {
            return;
        }
        if !self.sources.insert(source) {
            return;
        }
        self.samples.push(sample);

        let mut offsets = self.samples.clone();
        let count = offsets.len();

        offsets.sort_unstable();

        // Don't adjust if less than 5 samples exist.
        if count < MIN_TIME_SAMPLES {
            return;
        }

        // Only adjust when a true median is found.
        //
        // Note that this means the offset will *not* be adjusted when the last sample
        // is added, since `MAX_TIME_SAMPLES` is even. This is a known "bug" in Bitcoin Core
        // and we reproduce it here, since this code affects consensus.
        if count % 2 == 1 {
            let median_offset: TimeOffset = offsets[count / 2];

            // Don't let other nodes change our time by more than a certain amount.
            if median_offset.abs() <= MAX_TIME_ADJUSTMENT {
                self.offset = median_offset;
            } else {
                // TODO: Check whether other nodes have times similar to ours, otherwise
                // log a warning about our clock possibly being wrong.
                self.offset = 0;
            }
            #[cfg(feature = "log")]
            tracing::debug!("Time offset adjusted to {} seconds", self.offset);
        };
    }

    /// Set the local time to the given value.
    pub fn set_local_time(&mut self, time: LocalTime) {
        self.local_time = time;
    }

    /// Get the last known local time.
    pub fn local_time(&self) -> LocalTime {
        self.local_time
    }
}
