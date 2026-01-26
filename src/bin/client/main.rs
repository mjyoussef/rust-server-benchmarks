mod closed_loop;
mod open_loop;
mod partial_open_loop;

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
    #[arg(short, long)]
    runtime: u64,

    /// IP address of the server.
    #[arg(short, long)]
    ip: Ipv4Addr,

    /// Port of the server.
    #[arg(short, long)]
    port: u16,

    /// The directory to write results to.
    #[arg(short, long)]
    dir: PathBuf,

    /// Used for open and partial open loop request generators.
    #[arg(short, long)]
    delay: u64,

    /// Used for the the closed loop request generator.
    #[arg(short, long)]
    n_clients: usize,

    /// Used for the partial open loop request generator.
    #[arg(short, long)]
    max_clients: usize,

    /// Used for the partial open loop request generator.
    #[arg(short, long)]
    n_requests: usize,

    /// The workload type.
    #[command(subcommand)]
    work: Work,
}

#[derive(Clone, Debug, ValueEnum)]
enum Kind {
    Closed,
    Open,
    Partial,
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
                n_clients: args.n_clients,
            };
            let lrs = cfg.run();
            let n_reqs = lrs.len();
            let path = dir.join("closed/stats.txt");
            write_stats(lrs, n_reqs, args.runtime, &path).unwrap();
        }
        Kind::Open => {
            let cfg = open_loop::Config {
                addr,
                runtime,
                delay,
                work: args.work,
                n_clients: args.n_clients,
            };
            let (n_reqs, lrs) = cfg.run();
            let path = dir.join("open/stats.txt");
            write_stats(lrs, n_reqs, args.runtime, &path).unwrap();
        }
        Kind::Partial => {
            let cfg = partial_open_loop::Config {
                addr,
                runtime,
                delay,
                work: args.work,
                max_clients: args.max_clients,
                n_requests: args.n_requests,
            };

            let lrs = cfg.run();
            let n_reqs = lrs.len();
            let path = dir.join("partial/stats.txt");
            write_stats(lrs, n_reqs, args.runtime, &path).unwrap();
        }
    };
}
