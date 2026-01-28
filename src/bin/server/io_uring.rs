use std::{
    io,
    net::{SocketAddrV4, TcpListener, TcpStream},
    os::fd::AsRawFd,
};

use crossbeam_channel::{Receiver, unbounded};
use io_uring::{IoUring, opcode, types};
use nix::libc;
use rust_server_benchmarks::protocol::{REQUEST_SIZE, RESPONSE_SIZE, Request, Response};

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

struct Connection {
    /// The TCP connection.
    stream: Option<TcpStream>,

    /// The action being performed on the connection.
    action: Action,

    /// Reusable buffer for reading/writing on the connection.
    buf: Vec<u8>,

    /// Index into the buffer for reading or writing.
    idx: usize,
}

impl Connection {
    fn new() -> Self {
        let mut buf = Vec::with_capacity(REQUEST_SIZE.max(RESPONSE_SIZE));
        unsafe {
            buf.set_len(REQUEST_SIZE);
        }

        Self {
            stream: None,
            action: Action::Read,
            buf,
            idx: 0,
        }
    }

    fn init(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
        self.action = Action::Read;
        unsafe {
            self.buf.set_len(REQUEST_SIZE);
        }
        self.idx = 0;
    }

    fn drop(&mut self) {
        self.stream = None;
    }

    fn switch(&mut self) {
        match self.action {
            Action::Read => {
                self.action = Action::Write;
                unsafe {
                    self.buf.set_len(RESPONSE_SIZE);
                }
            }
            Action::Write => {
                self.action = Action::Read;
                unsafe {
                    self.buf.set_len(REQUEST_SIZE);
                }
            }
        }

        self.idx = 0;
    }

    fn handle_io_success(&mut self, result: usize) -> bool {
        self.idx += result as usize;
        self.idx == self.buf.len()
    }

    fn deserialize_request(&self) -> io::Result<Request> {
        Request::deserialize(&self.buf[..REQUEST_SIZE])
    }

    fn serialize_response(&mut self, response: Response) {
        response.serialize(&mut self.buf[..RESPONSE_SIZE]);
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

    /// Connection pool
    conns: Box<[Connection]>,

    /// Indices of entries that are free to reuse.
    free_conns: Vec<usize>,

    /// Read side of the connections channel.
    rx: Receiver<TcpStream>,
}

impl IOUringThread {
    fn new(ring_capacity: usize, batch_size: usize, rx: Receiver<TcpStream>) -> Self {
        let ring = IoUring::new(ring_capacity as u32).unwrap();
        let conns = (0..ring_capacity)
            .map(|_| Connection::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let free_conns = (0..ring_capacity).collect();

        Self {
            ring,
            ring_capacity,
            ring_len: 0,
            batch_size,
            conns,
            free_conns,
            rx,
        }
    }

    fn run(mut self) {
        // Prime the pipeline:
        // Queue as many reads as possible. Note that we must have at least one
        // connection, so we'll do a blocking `recv` when `idx == 0`.
        for idx in 0..self.free_conns.len() {
            let stream = if idx == 0 {
                self.rx.recv().unwrap()
            } else {
                match self.rx.try_recv() {
                    Ok(stream) => stream,
                    _ => break,
                }
            };

            // Get an `EntryData` item for the connection
            let conn_idx = self.free_conns.pop().expect("no entries available");
            let conn = &mut self.conns[conn_idx];

            // Push the submission queue entry
            let recv_sqe = opcode::Recv::new(
                types::Fd(stream.as_raw_fd()),
                conn.buf.as_mut_ptr(),
                REQUEST_SIZE as u32,
            )
            .build()
            .user_data(conn_idx as u64);
            unsafe {
                self.ring.submission().push(&recv_sqe).unwrap();
            }

            // Initialize the connection
            conn.init(stream);
        }

        // Submit and wait for `min(batch_size, ring_len)` entries to complete.
        let mut batch_size = self.batch_size.min(self.ring_len);
        let mut n_cqes = self.ring.submit_and_wait(batch_size).unwrap();

        // Pipeline:
        loop {
            // Drain the completion queue and handle each IO result
            while let Some(cqe) = self.ring.completion().next() {
                let conn = &mut self.conns[cqe.user_data() as usize];

                let result = cqe.result();
                if result > 0 {
                    if conn.handle_io_success(result as usize) {
                        // IO transfer is done
                        // TODO
                    } else {
                        // More bytes to read/write
                        // TODO
                    }
                } else if result == 0 {
                    // We've reached the end of the file -> drop the connection
                    // TODO
                } else if result == libc::EINTR {
                    // The system call was interrupted -> retry
                    // TODO
                } else {
                    // Unrecoverable error -> drop the connection
                    // TODO
                }
            }

            // Queue new ops, accepting new connections as needed ONLY if available in
            // `rx` and or `queue_size == 0`.
        }
    }
}
