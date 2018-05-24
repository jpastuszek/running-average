use std::collections::VecDeque;
use std::time::{Instant, Duration};

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
pub struct RunningAverage<TS: TimeSource> {
    window: VecDeque<u64>,
    front: TS::Instant,
    duration: Duration,
    time_source: TS,
}

impl RunningAverage<RealTimeSource> {
    pub fn new(duration: Duration, capacity: usize) -> RunningAverage<RealTimeSource> {
        RunningAverage::with_time_source(duration, capacity, RealTimeSource)
    }
}

impl<TS: TimeSource> RunningAverage<TS> {
    pub fn with_time_source(duration: Duration, capacity: usize, time_source: TS) -> RunningAverage<TS> {
        RunningAverage {
            window: (0..capacity).map(|_| 0).collect(),
            front: time_source.now(),
            duration: duration,
            time_source,
        }
    }
    
    fn shift(&mut self) {
        let now = self.time_source.now();
        let slot_duration = self.duration / self.window.len() as u32;

        // TODO: stop if we zeroed all slots or this can loop for long time if shift was not recently
        while now.duration_since(self.front) >= slot_duration {
            self.window.pop_back();
            self.window.push_front(0);
            self.front.forward(slot_duration);
        }
    }
    
    pub fn insert(&mut self, val: u64) {
        self.shift();
        *self.window.front_mut().unwrap() += val;
    }
    
    /// Sum of window in duration
    pub fn measure(&mut self) -> u64 {
        self.shift();
        self.window.iter().sum()
    }

    // TODO: mesure per second

    pub fn time_source(&mut self) -> &mut TS {
        &mut self.time_source
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
        }
    }

    #[test]
    fn const_half_time_over_different_capacity_real_time() {
        use super::*;

        for capacity in 1..31 {
            let mut tw = RunningAverage::<RealTimeSource>::new(Duration::from_secs(4), capacity);

            tw.insert(10);
            tw.insert(10);

            // TODO: this may fail?
            assert_eq!(tw.measure(), 20, "for capacity {}: {:?}", capacity, tw);
        }
    }
}
