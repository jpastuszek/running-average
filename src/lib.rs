use std::collections::VecDeque;
use std::time::{Instant, Duration};

pub trait TimeInstant {
    fn duration_since(&self, since: Self) -> f64;
    fn forward(&mut self, seconds: f64);
}

pub trait TimeSource {
    type Instant: TimeInstant + Copy;
    fn now(&self) -> Self::Instant;
}

fn dts(duration: Duration) -> f64 {
    duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9
}

fn std(seconds: f64) -> Duration {
    Duration::new(seconds.floor() as u64, ((seconds - seconds.floor()) * 1e-9) as u32)
}

impl TimeInstant for Instant {
    fn duration_since(&self, earlier: Self) -> f64 {
        dts(self.duration_since(earlier))
    }

    fn forward(&mut self, seconds: f64) {
        *self += std(seconds);
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

impl TimeInstant for f64 {
    fn duration_since(&self, earlier: Self) -> f64 {
        self - earlier
    }

    fn forward(&mut self, seconds: f64) {
        *self += seconds;
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
    duration: f64,
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
            duration: dts(duration),
            time_source,
        }
    }

    fn slot_duration(&self) -> f64 {
        self.duration / self.window.len() as f64
    }
    
    fn shift(&mut self) {
        let now = self.time_source.now();
        let shift_slots = (now.duration_since(self.front) / self.slot_duration()).floor() as usize;
        for _ in 0..shift_slots {
            self.window.pop_back();
            self.window.push_front(0);
        }
        let shift_slots_duration = shift_slots as f64 * self.slot_duration();
        self.front.forward(shift_slots_duration);
    }
    
    pub fn insert(&mut self, val: u64) {
        self.shift();
        *self.window.front_mut().unwrap() += val;
    }
    
    pub fn measure(&mut self) -> f64 {
        self.shift();
        self.window.iter().fold(0f64, |ret, val| {
            ret + *val as f64 / self.slot_duration() / self.window.len() as f64
        })
    }

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

            assert_eq!(tw.measure(), 10.0, "for capacity {}: {:?}", capacity, tw);
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

            assert_eq!(tw.measure(), 5.0, "for capacity {}: {:?}", capacity, tw); //TODO: don't eq floats
        }
    }
}
