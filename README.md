Rust crate that provides `RunningAverage` and `RealTimeRunningAverage` types that allow to calculate running averages with specified time window width using constant memory.

The `RunningAverage` type can be used when processing streams of temporal data while `RealTimeRunningAverage` can be used when measured events are happening in real time.

For example `RealTimeRunningAverage` can be used to measure download throughput by inserting how many bytes were transferred.
```
use running_average::RealTimeRunningAverage;

// By default use 8 second window with 16 accumulators
let mut tw = RealTimeRunningAverage::default();

// Connect and start downloading
// Got 2KB of data
tw.insert(2000);

// Waiting for more data
// Got 1KB of data
tw.insert(1000);

// Print average transfer for last 8 seconds
println!("{}", tw.measurement());
```