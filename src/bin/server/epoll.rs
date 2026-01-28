use std::{
    io::{self, Read, Write},
    net::{SocketAddrV4, TcpListener, TcpStream},
};

use nix::sys::*;

use crossbeam_channel::{Receiver, unbounded};
use rust_server_benchmarks::protocol::{REQUEST_SIZE, RESPONSE_SIZE, Request, Response};

pub fn run(addr: SocketAddrV4, n_threads: usize, capacity: usize, max_events: usize) {
    let listener = TcpListener::bind(addr).unwrap();
    let (tx, rx) = unbounded();
    println!("Server listening at {}", addr);

    // Start each epoll thread
    for _ in 0..n_threads {
        let rx = rx.clone();
        std::thread::spawn(move || {
            EpollThread::new(capacity, max_events, rx).run();
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
    /// The connection stream.
    stream: Option<TcpStream>,

    /// A reusable buffer for reading from and writing to the client.
    /// This is wrapped in a `Cursor` so that `buf` implements the `io::{Read, Write}`
    /// traits, which are neccessary for the serialization/deserialization logic.
    buf: Vec<u8>,

    /// The current index into the buffer for reading or writing.
    idx: usize,

    /// The action being performed on the connection.
    action: Action,
}

impl Connection {
    fn new(stream: Option<TcpStream>) -> Self {
        let mut buf = vec![0u8; REQUEST_SIZE.max(RESPONSE_SIZE)];
        unsafe { buf.set_len(REQUEST_SIZE) };

        Self {
            stream,
            buf,
            idx: 0,
            action: Action::Read,
        }
    }

    /// Initializes the stream for the connection.
    fn init(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
    }

    /// Drops the connection and prepares a new one for reading.
    fn reset(&mut self) {
        self.reinitialize(Action::Read);

        // Drop the connection
        self.stream = None;
    }

    /// Reinitializes a connection for a new action.
    fn reinitialize(&mut self, state: Action) {
        let new_buf_len = match state {
            Action::Read => REQUEST_SIZE,
            Action::Write => RESPONSE_SIZE,
        };

        unsafe { self.buf.set_len(new_buf_len) };
        self.idx = 0;
        self.action = state;
    }

    /// Copies bytes from the stream into the buffer or vice-versa (depending on the action
    /// being performed for the connection).
    fn copy_until_blocked(&mut self) -> io::Result<()> {
        let stream = self
            .stream
            .as_mut()
            .expect("cannot read/write from a stream that's uninitialized");

        let size = self.buf.len();

        loop {
            let result = match self.action {
                Action::Read => stream.read(&mut self.buf[self.idx..]),
                _ => stream.write(&mut self.buf[self.idx..]),
            };

            match result {
                Ok(0) => match self.action {
                    Action::Write => {
                        // This is problematic because we should've gotten a `WouldBlock` error.
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "unexpectedly wrote zero bytes",
                        ));
                    }
                    _ => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of file"));
                    }
                },
                Ok(n) => {
                    self.idx += n;

                    if self.idx == size {
                        break;
                    }
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::Interrupted => continue,
                    _ => {
                        return Err(e);
                    }
                },
            }
        }

        Ok(())
    }

    fn deserialize_request(&mut self) -> io::Result<Request> {
        Request::deserialize(&mut self.buf)
    }

    fn serialize_response(&mut self, response: Response) {
        response.serialize(&mut self.buf)
    }
}

struct Epoll {
    /// The Epoll file descriptor.
    epoll_fd: epoll::Epoll,

    /// Maximum number of concurrent connections the epoll thread can handle.
    capacity: usize,

    /// The pool of connection buffers.
    conns: Vec<Connection>,

    /// Indices of connections that may be used.
    free_conns: Vec<usize>,
}

impl Epoll {
    /// Creates a new `Epoll` instance.
    fn new(capacity: usize) -> Self {
        let epoll_fd = epoll::Epoll::new(epoll::EpollCreateFlags::empty()).unwrap();
        let conns = (0..capacity).map(|_| Connection::new(None)).collect();
        let free_conns = (0..capacity).collect();

        Self {
            epoll_fd,
            capacity,
            conns,
            free_conns,
        }
    }

    /// Adds a connection.
    fn add(&mut self, stream: TcpStream) -> io::Result<()> {
        let id = self
            .free_conns
            .pop()
            .expect("cannot add a connection while connection pool is full.");

        // Add an entry to the epoll fd's interest list.
        let event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, id as u64);
        self.epoll_fd.add(&stream, event)?;

        let conn = &mut self.conns[id];
        conn.init(stream);

        Ok(())
    }

    /// Deletes a connection.
    fn delete(&mut self, id: usize) -> io::Result<()> {
        let conn = &mut self.conns[id];
        let stream = conn.stream.as_ref().expect("connection not in use.");

        // Remove the stream from the epoll fd's interest list.
        self.epoll_fd.delete(stream)?;

        conn.reset();
        self.free_conns.push(id);

        Ok(())
    }

    /// Modifies the connection for a new action.
    fn modify(&mut self, id: usize, state: Action) -> io::Result<()> {
        let conn = &mut self.conns[id];
        let stream = conn.stream.as_ref().expect("connection not in use.");

        let event_flags = match state {
            Action::Read => epoll::EpollFlags::EPOLLIN,
            _ => epoll::EpollFlags::EPOLLOUT,
        };

        let mut event = epoll::EpollEvent::new(event_flags, id as u64);
        self.epoll_fd.modify(stream, &mut event)?;

        conn.reinitialize(state);

        Ok(())
    }

    /// Waits for file descriptors in the interest list to be ready.
    fn wait(&mut self, events: &mut [epoll::EpollEvent]) -> io::Result<usize> {
        let event_count = self.epoll_fd.wait(events, epoll::EpollTimeout::NONE)?;
        Ok(event_count)
    }

    /// Gets an immutable reference to a connection.
    #[allow(unused)]
    fn get_ref(&self, id: usize) -> &Connection {
        &self.conns[id]
    }

    /// Gets a mutable reference to a connection.
    fn get_mut(&mut self, id: usize) -> &mut Connection {
        &mut self.conns[id]
    }

    /// Returns `true` if there are no connections in use.
    fn is_empty(&self) -> bool {
        self.free_conns.len() == self.capacity
    }

    /// Returns `true` if the connection pool is at full capacity.
    fn is_full(&self) -> bool {
        self.free_conns.is_empty()
    }
}

struct EpollThread {
    /// The thread's `Epoll` instance.
    epoll: Epoll,

    /// Buffer of Epoll events used when calling `self.epoll.wait`.
    events: Vec<epoll::EpollEvent>,

    /// The receiving side of a channel of connections.
    rx_conn: Receiver<TcpStream>,
}

impl EpollThread {
    /// Creates a new `EpollThread`.
    ///
    /// # [Arguments]
    ///
    /// `capacity`   - the maximum number of concurrent connections.
    ///
    /// `max_events` - the maximum number of events it waits for per cycle.
    ///
    /// `rx_conn`    - the receiving side of a channel of connections.
    fn new(capacity: usize, max_events: usize, rx_conn: Receiver<TcpStream>) -> Self {
        Self {
            epoll: Epoll::new(capacity),
            events: vec![epoll::EpollEvent::empty(); max_events],
            rx_conn,
        }
    }

    /// Runs the event loop.
    fn run(mut self) {
        loop {
            // We must have at least one connection. If not, we'll block until we receive a connection.
            if self.epoll.is_empty() {
                let stream = self.rx_conn.recv().unwrap();
                self.epoll.add(stream).unwrap();
            }

            // Accept as many connections as possible
            while !self.epoll.is_full() {
                match self.rx_conn.try_recv() {
                    Ok(stream) => {
                        self.epoll.add(stream).unwrap();
                    }
                    _ => break,
                }
            }

            let event_count = self.epoll.wait(&mut self.events).unwrap();

            for i in 0..event_count {
                let event = self.events[i];
                self.events[i] = epoll::EpollEvent::empty();

                let id = event.data() as usize;
                let conn = self.epoll.get_mut(id);

                match conn.copy_until_blocked() {
                    Err(e) => {
                        if e.kind() == io::ErrorKind::WouldBlock {
                            continue;
                        }

                        self.epoll.delete(id).unwrap();
                    }
                    _ => match conn.action {
                        Action::Read => {
                            let response = conn.deserialize_request().unwrap().do_work();
                            conn.serialize_response(response);
                            self.epoll.modify(id, Action::Write).unwrap();
                        }
                        Action::Write => {
                            self.epoll.modify(id, Action::Read).unwrap();
                        }
                    },
                }
            }
        }
    }
}
