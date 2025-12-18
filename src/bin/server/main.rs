use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

use clap::{Parser, ValueEnum};

mod epoll;
mod io_uring;
mod threads;
mod vanilla;

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
    #[arg(long, default_value = "127.0.0.1")]
    ip: Ipv4Addr,

    /// Port to bind to
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

#[derive(Clone, Debug, ValueEnum)]
enum Kind {
    Epoll,
    IOUring,
    Threads,
    Vanilla,
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
        Kind::Threads => {
            todo!("not implemented")
        }
        Kind::Vanilla => {
            vanilla::run(addr);
        }
    });

    std::thread::sleep(timeout);
}
