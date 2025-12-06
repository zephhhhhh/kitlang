use std::time::Duration;
#[cfg(not(feature = "webasm"))]
use std::time::Instant;
#[cfg(feature = "webasm")]
use web_time::Instant;

use log::*;

use humanize_duration::{Truncate, prelude::*};

/// Measures the execution time of a provided lambda function (closure) and returns the result of
/// the provided lambda, along with the elapsed execution time.
pub fn measure_execution<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    (result, elapsed)
}

/// Converts a [`Duration`] into a human readable string.
pub fn format_duration(duration: Duration) -> String {
    duration.human(Truncate::Nano).to_string()
}

/// Measures the execution time of a provided lambda function (closure) and returns the result of
/// the provided lambda, prints the time elapsed while executing before returning the result.
pub fn print_execution_named<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let (result, duration) = measure_execution(f);
    debug!(
        "[Profiling] '{}' executed in {}.",
        name,
        format_duration(duration)
    );
    result
}
