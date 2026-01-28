use std::net::{SocketAddrV4, TcpListener, TcpStream};

use crossbeam_channel::{Receiver, unbounded};
use io_uring::IoUring;
use rust_server_benchmarks::protocol::{REQUEST_SIZE, RESPONSE_SIZE};

pub fn run(addr: SocketAddrV4, n_threads: usize, queue_size: usize, batch_size: usize) {
    let listener = TcpListener::bind(addr).unwrap();
    let (tx, rx) = unbounded();
    println!("Server listening at {}", addr);

    // Start each epoll thread
    for _ in 0..n_threads {
        let rx = rx.clone();
        std::thread::spawn(move || {
            IOUringThread::new(queue_size, batch_size, rx).run();
        });
    }

    // Accept connections
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        stream.set_nonblocking(true).unwrap();
        stream.set_nodelay(true).unwrap();
        tx.send(stream).unwrap();
    }
}

enum Action {
    Read,
    Write,
}

impl Action {
    #[inline]
    fn buf_len(&self) -> usize {
        match self {
            Action::Read => REQUEST_SIZE,
            Action::Write => RESPONSE_SIZE,
        }
    }
}

struct Entry {
    /// The TCP connection.
    stream: Option<TcpStream>,

    /// Reusable buffer for reading/writing on the connection.
    buf: Box<[u8]>,

    /// The action being performed on the connection.
    action: Action,
}

impl Entry {
    fn new() -> Self {
        todo!()
    }
}

struct IOUringThread {
    /// The io_uring file descriptor.
    ring: IoUring,

    /// The maximum number of entries (ie. connections) the ring can handle.
    queue_size: usize,

    /// The current number of entries being handled.
    curr_size: usize,

    /// The maximum number of entries that are drained from the completion queue
    /// in each polling cycle (see `IOUringThread::run` for more details).
    batch_size: usize,

    /// Entries for connections.
    entries: Box<[Entry]>,

    /// Indices of entries that are free to reuse.
    free_entries: Vec<usize>,

    /// Read side of the connections channel.
    rx: Receiver<TcpStream>,
}

impl IOUringThread {
    fn new(queue_size: usize, batch_size: usize, rx: Receiver<TcpStream>) -> Self {
        let ring = IoUring::new(queue_size as u32).unwrap();
        let entries = (0..queue_size)
            .map(|_| Entry::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let free_entries = (0..queue_size).collect();

        Self {
            ring,
            queue_size,
            curr_size: 0,
            batch_size,
            entries,
            free_entries,
            rx,
        }
    }

    fn run(self) {
        // Prime:
        // (1) Queue as many reads as possible
        // (2) Submit and wait for `min(batch_size, queue_size)`.

        // Loop:
        // (1) Drain `min(batch_size, queue_size)` from CQ and handle each
        // (2) Queue new ops, accepting new connections as needed ONLY if available in
        //     `rx` and or `queue_size == 0`.

        todo!()
    }
}
