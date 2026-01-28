use std::{
    net::{SocketAddrV4, TcpListener, TcpStream},
    os::fd::AsRawFd,
};

use crossbeam_channel::{Receiver, unbounded};
use io_uring::{IoUring, opcode, types};
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

struct EntryData {
    /// The TCP connection.
    stream: Option<TcpStream>,

    /// The action being performed on the connection.
    action: Action,

    /// Reusable buffer for reading/writing on the connection.
    buf: Vec<u8>,
}

impl EntryData {
    fn new() -> Self {
        Self {
            stream: None,
            action: Action::Read,
            buf: vec![0u8; REQUEST_SIZE.max(RESPONSE_SIZE)],
        }
    }

    fn init(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
        self.action = Action::Read;
    }
}

struct IOUringThread {
    /// The io_uring file descriptor.
    ring: IoUring,

    /// The maximum number of entries (ie. connections) the ring can handle.
    ring_capacity: usize,

    /// The current number of entries being handled.
    ring_len: usize,

    /// The maximum number of entries that are drained from the completion queue
    /// in each polling cycle (see `IOUringThread::run` for more details).
    batch_size: usize,

    /// Entries for connections.
    entries: Box<[EntryData]>,

    /// Indices of entries that are free to reuse.
    free_entries: Vec<usize>,

    /// Read side of the connections channel.
    rx: Receiver<TcpStream>,
}

impl IOUringThread {
    fn new(ring_capacity: usize, batch_size: usize, rx: Receiver<TcpStream>) -> Self {
        let ring = IoUring::new(ring_capacity as u32).unwrap();
        let entries = (0..ring_capacity)
            .map(|_| EntryData::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let free_entries = (0..ring_capacity).collect();

        Self {
            ring,
            ring_capacity,
            ring_len: 0,
            batch_size,
            entries,
            free_entries,
            rx,
        }
    }

    fn run(mut self) {
        // Prime the pipeline:
        // Queue as many reads as possible. Note that we must have at least one
        // connection, so we'll do a blocking `recv` when `idx == 0`.
        for idx in 0..self.ring_capacity {
            let stream = if idx == 0 {
                self.rx.recv().unwrap()
            } else {
                match self.rx.try_recv() {
                    Ok(stream) => stream,
                    _ => break,
                }
            };

            // Get an `EntryData` item for the connection
            let entry_idx = self.free_entries.pop().expect("no entries available");
            let entry = &mut self.entries[entry_idx];

            // Push the submission queue entry
            let recv_sqe = opcode::Recv::new(
                types::Fd(stream.as_raw_fd()),
                entry.buf.as_mut_ptr(),
                REQUEST_SIZE as u32,
            )
            .build()
            .user_data(entry_idx as u64);
            unsafe {
                self.ring.submission().push(&recv_sqe).unwrap();
            }

            // Initialize the `EntryData` item
            entry.init(stream);
        }

        // (2) Submit and wait for `min(batch_size, ring_len)` entries to complete.
        let batch_size = self.batch_size.min(self.ring_len);
        let mut n_cqes = self.ring.submit_and_wait(batch_size).unwrap();

        // Pipeline
        loop {
            // (1) Drain `min(batch_size, queue_size)` from CQ and handle each
            // (2) Queue new ops, accepting new connections as needed ONLY if available in
            //     `rx` and or `queue_size == 0`.
        }
    }
}
