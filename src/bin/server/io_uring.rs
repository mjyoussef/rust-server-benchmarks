use std::{
    io,
    net::{SocketAddrV4, TcpListener, TcpStream},
    os::fd::AsRawFd,
};

use crossbeam_channel::{Receiver, unbounded};
use io_uring::{IoUring, SubmissionQueue, opcode, types};
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

enum Operation {
    Read,
    Write,
}

struct Connection {
    /// The TCP connection.
    stream: Option<TcpStream>,

    /// The operation being performed on the connection.
    op: Operation,

    /// Buffer for reading/writing on the connection.
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
            op: Operation::Read,
            buf,
            idx: 0,
        }
    }

    /// Initializes a connection with the specified operation.
    fn init(&mut self, stream: TcpStream, op: Operation) {
        self.stream = Some(stream);
        self.op = op;
        unsafe {
            self.buf.set_len(REQUEST_SIZE);
        }
        self.idx = 0;
    }

    /// Drops the connection. This method doesn't reinitialize the connection state.
    /// If you need to handle a new connection, you'll need to call `init`.
    fn drop(&mut self) {
        self.stream = None;
    }

    /// Switches the action and reinitializes the connection state. If the current action
    /// is reading, then the connection is prepared for writing (and vice-versa).
    fn switch(&mut self) {
        match self.op {
            Operation::Read => {
                self.op = Operation::Write;
                unsafe {
                    self.buf.set_len(RESPONSE_SIZE);
                }
            }
            Operation::Write => {
                self.op = Operation::Read;
                unsafe {
                    self.buf.set_len(REQUEST_SIZE);
                }
            }
        }

        self.idx = 0;
    }

    /// Handles a successful IO operation. This method returns `true` if the operation
    /// is complete and `false` if only a partial # of bytes were read/written.
    fn handle_io_success(&mut self, result: usize) -> bool {
        self.idx += result;
        self.idx == self.buf.len()
    }

    /// Deserializes a request from the connection's buffer.
    fn deserialize_request(&self) -> io::Result<Request> {
        Request::deserialize(&self.buf[..REQUEST_SIZE])
    }

    /// Serializes a response into the connection's buffer.
    fn serialize_response(&mut self, response: Response) {
        response.serialize(&mut self.buf[..RESPONSE_SIZE]);
    }

    /// Returns a pointer to the connection buffer.
    fn get_buf_ptr(&mut self) -> *mut u8 {
        self.buf[self.idx..].as_mut_ptr()
    }

    /// Returns the length of the connection buffer.
    fn get_buf_len(&self) -> u32 {
        self.buf[self.idx..].len() as u32
    }
}

struct IOUringThread {
    /// The io_uring file descriptor.
    ring: IoUring,

    /// The maximum number of entries (ie. connections) the ring can handle.
    ring_capacity: usize,

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
            batch_size,
            conns,
            free_conns,
            rx,
        }
    }

    fn run(mut self) {
        loop {
            self.accept_conns();
            self.submit_wait_batch();

            // We're using `split` to avoid mutable borrow conflict that happens when draining the CQ
            // while pushing entries to the SQ.
            let (_, mut sq, mut cq) = self.ring.split();

            // Might need this..
            // sq.sync();
            // cq.sync();

            // Drain the completion queue and handle each IO result
            while let Some(cqe) = cq.next() {
                let conn_idx = cqe.user_data() as usize;
                let conn = &mut self.conns[conn_idx];

                let result = cqe.result();
                if result > 0 {
                    if conn.handle_io_success(result as usize) {
                        // IO transfer is done -> queue new IO operation
                        match conn.op {
                            Operation::Read => {
                                let request = match conn.deserialize_request() {
                                    Ok(request) => request,
                                    Err(e) => {
                                        // Log error and drop the connection
                                        eprintln!("{e}");
                                        conn.drop();
                                        self.free_conns.push(conn_idx);
                                        continue;
                                    }
                                };
                                let response = request.do_work();
                                conn.switch();
                                conn.serialize_response(response);
                                IOUringThread::queue_write(&mut sq, conn, conn_idx);
                            }
                            Operation::Write => {
                                conn.switch();
                                IOUringThread::queue_read(&mut sq, conn, conn_idx);
                            }
                        }
                    } else {
                        // More bytes to read/write -> retry
                        match conn.op {
                            Operation::Read => {
                                IOUringThread::queue_read(&mut sq, conn, conn_idx);
                            }
                            Operation::Write => {
                                IOUringThread::queue_write(&mut sq, conn, conn_idx);
                            }
                        }
                    }
                } else if result == libc::EINTR {
                    // The system call was interrupted -> retry
                    match conn.op {
                        Operation::Read => {
                            IOUringThread::queue_read(&mut sq, conn, conn_idx);
                        }
                        Operation::Write => {
                            IOUringThread::queue_write(&mut sq, conn, conn_idx);
                        }
                    }
                } else if result <= 0 {
                    // We've reached the end of the file or there was an unrecoverable
                    // error -> drop the connection
                    conn.drop();
                    self.free_conns.push(conn_idx);
                }
            }
        }
    }

    fn ring_len(&self) -> usize {
        self.ring_capacity - self.free_conns.len()
    }

    /// Queues new connections while the ring has space and either the ring is empty
    /// or receiving from `self.rx` doesn't block.
    fn accept_conns(&mut self) {
        for _ in 0..self.free_conns.len() {
            let stream = if self.ring_len() == 0 {
                self.rx.recv().unwrap()
            } else {
                match self.rx.try_recv() {
                    Ok(stream) => stream,
                    _ => break,
                }
            };

            // Initialize the connection
            let conn_idx = self.free_conns.pop().expect("no entries available");
            let conn = &mut self.conns[conn_idx];
            conn.init(stream, Operation::Read);

            // Push the submission queue entry
            IOUringThread::queue_read(&mut self.ring.submission(), conn, conn_idx);
        }
    }

    /// Submits and waits for a batch of IO operations to complete. The size
    /// of the batch is the minimum of `self.batch_size` and the current length
    /// of the ring.
    fn submit_wait_batch(&self) {
        let batch_size = self.batch_size.min(self.ring_len());
        self.ring.submit_and_wait(batch_size).unwrap();
    }

    /// Queues a read operation for a connection.
    fn queue_read(sq: &mut SubmissionQueue<'_>, conn: &mut Connection, conn_idx: usize) {
        let stream = conn.stream.as_ref().unwrap();

        let recv_sqe = opcode::Recv::new(
            types::Fd(stream.as_raw_fd()),
            conn.get_buf_ptr(),
            conn.get_buf_len(),
        )
        .build()
        .user_data(conn_idx as u64);

        unsafe {
            sq.push(&recv_sqe).unwrap();
        }
    }

    /// Queues a write operation for a connection.
    fn queue_write(sq: &mut SubmissionQueue<'_>, conn: &mut Connection, conn_idx: usize) {
        let stream = conn.stream.as_ref().unwrap();

        let recv_sqe = opcode::Send::new(
            types::Fd(stream.as_raw_fd()),
            conn.get_buf_ptr(),
            conn.get_buf_len(),
        )
        .build()
        .user_data(conn_idx as u64);

        unsafe {
            sq.push(&recv_sqe).unwrap();
        }
    }
}
