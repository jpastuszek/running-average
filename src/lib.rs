use std::collections::VecDeque;
use std::time::{Instant, Duration};
use std::ops::AddAssign;
use std::iter::Sum;
use std::default::Default;

pub trait TimeInstant {
    fn duration_since(&self, since: Self) -> Duration;
    fn forward(&mut self, duration: Duration);
}

pub trait TimeSource {
    type Instant: TimeInstant + Copy;
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

#[derive(Debug)]
pub struct RunningAverage<V: Default, TS: TimeSource = RealTimeSource> {
    window: VecDeque<V>,
    front: TS::Instant,
    duration: Duration,
    time_source: TS,
}

impl<V: Default> RunningAverage<V, RealTimeSource> {
    pub fn new(duration: Duration) -> RunningAverage<V, RealTimeSource> {
        RunningAverage::with_capacity(duration, 16)
    }

    pub fn with_capacity(duration: Duration, capacity: usize) -> RunningAverage<V, RealTimeSource> {
        RunningAverage::with_time_source(duration, capacity, RealTimeSource)
    }
}

impl<V: Default> Default for RunningAverage<V, RealTimeSource> {
    fn default() -> RunningAverage<V, RealTimeSource> {
        RunningAverage::new(Duration::from_secs(8))
    }
}

impl<V: Default, TS: TimeSource> RunningAverage<V, TS> {
    pub fn with_time_source(duration: Duration, capacity: usize, time_source: TS) -> RunningAverage<V, TS> {
        RunningAverage {
            window: (0..capacity).map(|_| V::default()).collect(),
            front: time_source.now(),
            duration: duration,
            time_source,
        }
    }
    
    fn shift(&mut self) {
        let now = self.time_source.now();
        let slot_duration = self.duration / self.window.len() as u32;

        let mut slots_to_go = self.window.len();
        while now.duration_since(self.front) >= slot_duration {
            // Stop if we zeroed all slots or this can loop for long time if shift was not called recently
            if slots_to_go == 0 {
                let since_front = now.duration_since(self.front);
                self.front.forward(since_front);
                break;
            }
            self.window.pop_back();
            self.window.push_front(V::default());
            self.front.forward(slot_duration);
            slots_to_go -= 1;
        }
    }
    
    pub fn insert(&mut self, val: V) where V: AddAssign<V> {
        self.shift();
        *self.window.front_mut().unwrap() += val;
    }
    
    /// Sum of window in duration
    pub fn measure<'i>(&'i mut self) -> V where V: Sum<&'i V> {
        self.shift();
        self.window.iter().sum()
    }

    pub fn measure_per_second<'i>(&'i mut self) -> <V as ToRate>::Output where V: Sum<&'i V> + ToRate {
        let ds = self.duration;
        self.measure().to_rate(ds)
    }

    pub fn time_source(&mut self) -> &mut TS {
        &mut self.time_source
    }
}

pub trait ToRate {
    type Output;
    fn to_rate(self, duration: Duration) -> Self::Output;
}

//Note: This is not implemented for u64 as it cannot be converted precisely to f64 - use f64 instead for big numbers
//Note: Duration can be converted to f64 but will be rounded to fit in it so it is not 100% precise for max Duration
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
            let mut tw = RunningAverage::with_time_source(Duration::from_secs(4), capacity, ManualTimeSource::new());

            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);

            assert_eq!(tw.measure(), 40, "for capacity {}: {:?}", capacity, tw);
            assert_eq!(tw.measure_per_second(), 10.0, "for capacity {}: {:?}", capacity, tw);
        }
    }

    #[test]
    fn const_half_time_over_different_capacity() {
        use super::*;

        for capacity in 1..31 {
            let mut tw = RunningAverage::with_time_source(Duration::from_secs(4), capacity, ManualTimeSource::new());

            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.insert(10);
            tw.time_source().time_shift(1.0);
            tw.time_source().time_shift(1.0);

            assert_eq!(tw.measure(), 20, "for capacity {}: {:?}", capacity, tw);
            assert_eq!(tw.measure_per_second(), 5.0, "for capacity {}: {:?}", capacity, tw);
        }
    }

    #[test]
    fn default_int() {
        use super::*;

        let mut tw = RunningAverage::default();

        tw.insert(10);
        tw.insert(10);

        // Note: this may fail as it is based on real time
        assert_eq!(tw.measure(), 20, "default: {:?}", tw);
        assert_eq!(tw.measure_per_second(), 2.5, "default: {:?}", tw);
    }

    #[test]
    fn long_time_shift() {
        use super::*;

        let mut tw = RunningAverage::with_time_source(Duration::from_secs(4), 16, ManualTimeSource::new());

        tw.insert(10);
        tw.time_source().time_shift(1_000_000_000.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);
        tw.time_source().time_shift(1.0);
        tw.insert(10);

        assert_eq!(tw.measure(), 40, "long: {:?}", tw);
        assert_eq!(tw.measure_per_second(), 10.0, "long: {:?}", tw);
    }

    #[test]
    fn default_f64() {
        use super::*;

        let mut tw = RunningAverage::default();

        tw.insert(10f64);
        tw.insert(10.0);

        // Note: this may fail as it is based on real time
        assert_eq!(tw.measure(), 20.0, "default: {:?}", tw);
        assert_eq!(tw.measure_per_second(), 2.5, "default: {:?}", tw);
    }
}
