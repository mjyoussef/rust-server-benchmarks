use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

use clap::{Parser, ValueEnum};

mod epoll;
mod io_uring;
mod threadpool;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The type of server
    #[arg(short, long)]
    kind: Kind,

    /// Timeout in seconds
    #[arg(short, long, default_value_t = 24)]
    timeout: u64,

    /// IP address to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: Ipv4Addr,

    /// Port to bind to
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Threadpool size (ignored for epoll, io_uring servers)
    #[arg(short, long, default_value_t = 16)]
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
