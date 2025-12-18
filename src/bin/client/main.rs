mod closed_loop;
mod open_loop;

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

use clap::{Parser, ValueEnum};
use rust_server_benchmarks::{protocol::Work, write_stats};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The type of server.
    #[arg(short, long)]
    kind: Kind,

    /// Timeout in seconds.
    #[arg(short, long, default_value_t = 6)]
    runtime: u64,

    /// Delay in microseconds. This argument is ignored if using
    /// the closed loop request generator.
    #[arg(short, long)]
    delay: u64,

    /// IP address of the server.
    #[arg(long, default_value = "127.0.0.1")]
    ip: Ipv4Addr,

    /// Port of the server.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// The number of clients. This argument is ignored if using
    /// the open loop request generator.
    #[arg(long, default_value_t = 1)]
    num_clients: u16,

    /// Directory to write results to
    #[arg(short, long)]
    dir: PathBuf,

    /// The workload type.
    #[command(subcommand)]
    work: Work,
}

#[derive(Clone, Debug, ValueEnum)]
enum Kind {
    Closed,
    Open,
}

fn main() {
    let args = Args::parse();
    let addr = SocketAddrV4::new(args.ip, args.port);
    let runtime = Duration::from_secs(args.runtime);
    let delay = Duration::from_micros(args.delay);
    let dir = args.dir;

    match args.kind {
        Kind::Closed => {
            let cfg = closed_loop::Config {
                addr,
                runtime,
                work: args.work,
                num_clients: 1,
            };
            let lrs = cfg.run();
            let n_reqs = lrs.len();
            let path = dir.join("closed/stats.txt");
            println!("{:?}", path);
            write_stats(lrs, n_reqs, None, args.runtime, &path).unwrap();
        }
        Kind::Open => {
            let cfg = open_loop::Config {
                addr,
                runtime,
                delay,
                work: args.work,
            };
            let (n_reqs, lrs) = cfg.run();
            let path = dir.join("open/stats.txt");
            write_stats(lrs, n_reqs, Some(args.delay), args.runtime, &path).unwrap();
        }
    };
}
