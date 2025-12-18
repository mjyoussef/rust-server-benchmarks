pub mod protocol;

use std::{
    fs::{self, File},
    io::{Result, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::protocol::LatencyRecord;

/// Gets the current time (in nanoseconds) since the UNIX epoch.
pub fn get_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Saves performance statistics.
///
/// # Arguments
///
/// * `lrs` - The latency records.
/// * `n` - Number of requests sent (this should match `lrs.len()` for a closed
///    loop request generator).
/// * `runtime` - Total runtime in microseconds.
/// * `path` - The destination file path.
pub fn write_stats(lrs: Vec<LatencyRecord>, n: usize, runtime: u64, path: &PathBuf) -> Result<()> {
    // Calculate the 50, 95, and 99th percentile latencies
    let mut latencies: Vec<_> = lrs.iter().map(|lr| lr.recv_time - lr.send_time).collect();

    latencies.sort();
    let p_50 = latencies[latencies.len() / 2] as f64 / 1000.0;
    let p_95 = latencies[(latencies.len() as f64 * 0.95 as f64) as usize] as f64 / 1000.0;
    let p_99 = latencies[(latencies.len() as f64 * 0.99 as f64) as usize] as f64 / 1000.0;

    // Calculate the attempted, offered, and achieved throughput
    let offered = n as u64 / runtime;
    let achieved = latencies.len() as u64 / runtime;

    fs::create_dir_all(path.parent().expect("file path is missing directory"))?;
    let mut file = File::create(path).unwrap();

    writeln!(file, "{p_50}, {p_95}, {p_99}")?;
    writeln!(file, "{offered}, {achieved}")?;

    Ok(())
}
