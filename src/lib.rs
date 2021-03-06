//! `RunningAverage` and `RealTimeRunningAverage` types allow to calculate running average with specified time window width using constant memory.
//! 
//! The `RunningAverage` type can be used when processing streams of temporal data while `RealTimeRunningAverage` can be used when measured events are happening in real time.
//! 
//! For example `RealTimeRunningAverage` can be used to measure download throughput by inserting how many bytes were transferred.
//! ```
//! use running_average::RealTimeRunningAverage;
//! 
//! // By default use 8 second window with 16 accumulators
//! let mut tw = RealTimeRunningAverage::default();
//! 
//! // Connect and start downloading
//! // Got 2KB of data
//! tw.insert(2000);
//! 
//! // Waiting for more data
//! // Got 1KB of data
//! tw.insert(1000);
//! 
//! // Print average transfer for last 8 seconds
//! println!("{}", tw.measurement());
//! ```

use std::collections::VecDeque;
use std::time::{Instant, Duration};
use std::ops::AddAssign;
use std::iter::Sum;
use std::default::Default;

/// Types implementing this trait can be used as Instant type in TimeSource trait and for RunningAverage
pub trait TimeInstant {
    /// Returns Duration elapsed since given TimeInstant and Self.
    fn duration_since(&self, since: Self) -> Duration;
    /// Forward Self by given Duration into future.
    fn forward(&mut self, duration: Duration);
}

/// Types implementing this trait can be used as TimeSource for RealTimeRunningAverage.
pub trait TimeSource {
    /// Type implementing TimeInstant for this TimeSource.
    type Instant: TimeInstant + Copy;
    /// Return current Instant.
    fn now(&self) -> Self::Instant;
}

impl TimeInstant for Instant {
    fn duration_since(&self, earlier: Self) -> Duration {
        self.duration_since(earlier)
    }

    fn forward(&mut self, duration: Duration) {
        *self += duration;
    }
}

/// TimeSource that uses real time clock via `Instant::now()`.
#[derive(Debug)]
pub struct RealTimeSource;
impl TimeSource for RealTimeSource {
    type Instant = Instant;

    fn now(&self) -> Self::Instant {
        Instant::now()
    }
}

fn dts(duration: Duration) -> f64 {
    duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9
}

fn std(seconds: f64) -> Duration {
    assert!(seconds >= 0.0, "RunningAverage negative duration - time going backwards?");
    Duration::new(seconds.floor() as u64, ((seconds - seconds.floor()) * 1e-9) as u32)
}

impl TimeInstant for f64 {
    fn duration_since(&self, earlier: Self) -> Duration {
        std(self - earlier)
    }

    fn forward(&mut self, duration: Duration) {
        *self += dts(duration);
    }
}

/// TimeSource that has to be manually progressed forward via `ManualTimeSource::time_shift()` method.
#[derive(Debug)]
pub struct ManualTimeSource {
    now: f64,
}

impl TimeSource for ManualTimeSource {
    type Instant = f64;

    fn now(&self) -> Self::Instant {
        self.now
    }
}

impl ManualTimeSource {
    pub fn new() -> ManualTimeSource {
        ManualTimeSource {
            now: 0.0
        }
    }

    pub fn time_shift(&mut self, seconds: f64) {
        self.now += seconds;
    }
}

/// Represent result of the calculation of running average
#[derive(Debug)]
pub struct Measurement<T> {
    value: T, 
    duration: Duration,
}

use std::fmt;
impl<T> fmt::Display for Measurement<T> where T: Clone + fmt::Display + ToRate, <T as ToRate>::Output: Into<f64> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:.3}", self.rate().into())
    }
}

impl<T> Measurement<T> {
    /// Returns pointer to internal value of the measurement which is sum of all samples within time window
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns internal value of the measurement which is sum of all samples within time window consuming self
    pub fn unwrap(self) -> T {
        self.value
    }

    /// Calculates actual running average value based on sum of all samples and width of the time window
    pub fn rate(&self) -> <T as ToRate>::Output where T: Clone + ToRate {
        self.value.clone().to_rate(self.duration)
    }

    /// Calculates actual running average value based on sum of all samples and width of the time window consuming self
    pub fn to_rate(self) -> <T as ToRate>::Output where T: ToRate {
        self.value.to_rate(self.duration)
    }
}

/// Represents running average calculation window.
/// It is using specified window width that will consist of given number of accumulator buckets to ensure constant memory usage.
#[derive(Debug)]
pub struct RunningAverage<V: Default, I: TimeInstant + Copy> {
    window: VecDeque<V>,
    front: Option<I>,
    duration: Duration,
}

impl<V: Default, I: TimeInstant + Copy> Default for RunningAverage<V, I> {
    /// Crate new RunningAverage instance with window of 8 seconds width and 16 buckets.
    fn default() -> RunningAverage<V, I> {
        RunningAverage::new(Duration::from_secs(8))
    }
}

impl<V: Default, I: TimeInstant + Copy> RunningAverage<V, I> {
    /// Crate new RunningAverage instance that will average over window of width of given duration using 16 buckets.
    pub fn new(duration: Duration) -> RunningAverage<V, I> {
        RunningAverage::with_capacity(duration, 16)
    }

    /// Crate new RunningAverage instance that will average over window of width of given duration with specific number of buckets to use.
    pub fn with_capacity(duration: Duration, capacity: usize) -> RunningAverage<V, I> {
        assert!(capacity > 0, "RunningAverage capacity cannot be 0");
        RunningAverage {
            window: (0..capacity).map(|_| V::default()).collect(),
            front: None,
            duration: duration,
        }
    }

    fn shift(&mut self, now: I) {
        let front = self.front.get_or_insert(now);
        let slot_duration = self.duration / self.window.len() as u32;
        let mut slots_to_go = self.window.len();

        while now.duration_since(*front) >= slot_duration {
            // Stop if we zeroed all slots or this can loop for long time if shift was not called recently
            if slots_to_go == 0 {
                let since_front = now.duration_since(*front);
                front.forward(since_front);
                break;
            }
            self.window.pop_back();
            self.window.push_front(V::default());
            front.forward(slot_duration);
            slots_to_go -= 1;
        }
    }
    
    /// Insert value to be average over at given time instant.
    /// Panics if now is less than previous now - time cannot go backwards
    pub fn insert(&mut self, now: I, val: V) where V: AddAssign<V> {
        self.shift(now);
        *self.window.front_mut().unwrap() += val;
    }

    /// Calculate running average using time window ending at given time instant.
    /// Panics if now is less than previous now - time cannot go backwards.
    pub fn measurement<'i>(&'i mut self, now: I) -> Measurement<V> where V: Sum<&'i V> {
        self.shift(now);

        Measurement {
            value: self.window.iter().sum(),
            duration: self.duration,
        }
    }
}

/// Represents running average calculation window where `shift` and `measurement` are using given time source to obtain value of `now` instant.
/// It is using specified window width that will consist of given number of accumulator buckets to ensure constant memory usage.
#[derive(Debug)]
pub struct RealTimeRunningAverage<V: Default, TS: TimeSource = RealTimeSource> {
    inner: RunningAverage<V, TS::Instant>,
    time_source: TS,
}

impl<V: Default> Default for RealTimeRunningAverage<V, RealTimeSource> {
    fn default() -> RealTimeRunningAverage<V, RealTimeSource> {
        RealTimeRunningAverage::new(Duration::from_secs(8))
    }
}

impl<V: Default> RealTimeRunningAverage<V, RealTimeSource> {
    /// Crate new instance with window of given width duration and using RealTimeSource as time source for `now` instant.
    /// Note: new() is parametrizing output to RealTimeSource as this cannot be inferred otherwise.
    pub fn new(duration: Duration) -> RealTimeRunningAverage<V, RealTimeSource> {
        let time_source = RealTimeSource;

        RealTimeRunningAverage {
            inner: RunningAverage::new(duration),
            time_source,
        }
    }
}

impl<V: Default, TS: TimeSource> RealTimeRunningAverage<V, TS> {
    /// Crate new instance with window of given width duration and using given as time source for `now` instant.
    pub fn with_time_source(duration: Duration, capacity: usize, time_source: TS) -> RealTimeRunningAverage<V, TS> {
        RealTimeRunningAverage {
            inner: RunningAverage::with_capacity(duration, capacity),
            time_source,
        }
    }

    /// Insert value to be average over now.
    /// Panics if time source time goes backwards.
    pub fn insert(&mut self, val: V) where V: AddAssign<V> {
        let now = self.time_source.now();
        self.inner.insert(now, val)
    }
    
    /// Calculate running average using time window ending now.
    /// Panics if time source time goes backwards.
    pub fn measurement<'i>(&'i mut self) -> Measurement<V> where V: Sum<&'i V> {
        let now = self.time_source.now();
        self.inner.measurement(now)
    }

    /// Return mutable reference to time source used.
    pub fn time_source(&mut self) -> &mut TS {
        &mut self.time_source
    }
}

/// Types implementing this trait can be used to calculate `Measurement::rate()` from.
/// Note: This is not implemented for u64 as it cannot be converted precisely to f64 - use f64 instead for big numbers
/// Note: Duration can be converted to f64 but will be rounded to fit in it so it is not 100% precise for max Duration
pub trait ToRate {
    type Output;
    fn to_rate(self, duration: Duration) -> Self::Output;
}

impl<T: Into<f64>> ToRate for T {
    type Output = f64;

    fn to_rate(self, duration: Duration) -> f64 {
        let v: f64 = self.into();
        v / dts(duration)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn const_over_different_capacity() {
        use super::*;

        for capacity in 1..31 {
            let mut tw = RealTimeRunningAverage::with_time_source(Duration::from_secs(4), capacity, ManualTimeSource::new());

            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);

            assert_eq!(tw.measurement().unwrap(), 40, "for capacity {}: {:?}", capacity, tw);
            assert_eq!(tw.measurement().to_rate(), 10.0, "for capacity {}: {:?}", capacity, tw);
        }
    }

    #[test]
    fn const_half_time_over_different_capacity() {
        use super::*;

        for capacity in 1..31 {
            let mut tw = RealTimeRunningAverage::with_time_source(Duration::from_secs(4), capacity, ManualTimeSource::new());

            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.time_source().time_shift(1.0);

            assert_eq!(tw.measurement().unwrap(), 20, "for capacity {}: {:?}", capacity, tw);
            assert_eq!(tw.measurement().to_rate(), 5.0, "for capacity {}: {:?}", capacity, tw);
        }
    }

    #[test]
    fn default_int() {
        use super::*;

        let mut tw = RealTimeRunningAverage::default();

        tw.insert(10);
        tw.insert(10);

        // Note: this may fail as it is based on real time
        assert_eq!(tw.measurement().unwrap(), 20, "default: {:?}", tw);
        assert_eq!(tw.measurement().to_rate(), 2.5, "default: {:?}", tw);
    }

    #[test]
    fn default_f64() {
        use super::*;

        let mut tw = RealTimeRunningAverage::default();

        tw.insert(10f64);
        tw.insert(10.0);

        // Note: this may fail as it is based on real time
        assert_eq!(tw.measurement().unwrap(), 20.0, "default: {:?}", tw);
        assert_eq!(tw.measurement().to_rate(), 2.5, "default: {:?}", tw);
    }

    #[test]
    fn long_time_shift() {
        use super::*;

        let mut tw = RealTimeRunningAverage::with_time_source(Duration::from_secs(4), 16, ManualTimeSource::new());

        tw.insert(10);
        tw.time_source().time_shift(1_000_000_000.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);

        assert_eq!(tw.measurement().unwrap(), 40, "long: {:?}", tw);
        assert_eq!(tw.measurement().to_rate(), 10.0, "long: {:?}", tw);
    }

    #[test]
    fn measurement_display() {
        use super::*;

        let mut tw = RealTimeRunningAverage::default();

        tw.insert(10);
        tw.insert(10);

        assert_eq!(&format!("{}", tw.measurement()), "2.500");
    }
}
