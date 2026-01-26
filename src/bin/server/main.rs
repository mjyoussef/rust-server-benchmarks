use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

use clap::{Parser, ValueEnum};

mod asynchronous;
mod epoll;
mod io_uring;
mod threadpool;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The type of server.
    #[arg(short, long)]
    kind: Kind,

    /// Timeout in seconds.
    #[arg(short, long)]
    timeout: u64,

    /// Server IP address.
    #[arg(short, long)]
    ip: Ipv4Addr,

    /// Server port.
    #[arg(short, long)]
    port: u16,

    /// Used for threadpool.rs.
    #[arg(short, long)]
    tp_size: usize,
}

#[derive(Clone, Debug, ValueEnum)]
enum Kind {
    Epoll,
    IOUring,
    ThreadPool,
}

fn main() {
    let args = Args::parse();
    let timeout = Duration::from_secs(args.timeout);
    let addr = SocketAddrV4::new(args.ip, args.port);

    std::thread::spawn(move || match args.kind {
        Kind::Epoll => {
            todo!("not implemented")
        }
        Kind::IOUring => {
            todo!("not implemented")
        }
        Kind::ThreadPool => {
            threadpool::run(addr, args.tp_size);
        }
    });

    std::thread::sleep(timeout);
}
