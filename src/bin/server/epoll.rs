use std::{
    io::{self, Cursor, Read, Write},
    net::{SocketAddrV4, TcpListener, TcpStream},
};

use nix::sys::*;

use crossbeam_channel::{Receiver, unbounded};
use rust_server_benchmarks::protocol::{
    Deserialize, REQUEST_SIZE, RESPONSE_SIZE, Request, Response, Serialize,
};

pub fn run(addr: SocketAddrV4, n_threads: usize, capacity: usize, max_events: usize) {
    let listener = TcpListener::bind(addr).unwrap();
    let (tx, rx) = unbounded::<TcpStream>();
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
    buf: Cursor<Vec<u8>>,

    /// The current index into the buffer for reading or writing.
    idx: usize,

    /// The action being performed on the connection.
    action: Action,
}

impl Connection {
    fn new(stream: Option<TcpStream>) -> Self {
        Self {
            stream,
            buf: Cursor::new(vec![0u8; REQUEST_SIZE]),
            idx: 0,
            action: Action::Read,
        }
    }

    fn init(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
    }

    fn reset(&mut self, state: Action) {
        match state {
            Action::Read => {
                self.buf.get_mut().resize(REQUEST_SIZE, 0);
            }
            Action::Write => {
                self.buf.get_mut().resize(RESPONSE_SIZE, 0);
            }
        }
        self.stream = None; // drop the connection
        self.buf.set_position(0);
        self.idx = 0;
        self.action = state;
    }

    fn copy_until_blocked(&mut self) -> io::Result<()> {
        let stream = self.stream.as_mut().unwrap();

        let size = match self.action {
            Action::Read => REQUEST_SIZE,
            _ => RESPONSE_SIZE,
        };

        loop {
            let result = match self.action {
                Action::Read => stream.read(&mut self.buf.get_mut()[self.idx..]),
                _ => stream.write(&mut self.buf.get_mut()[self.idx..]),
            };

            match result {
                Ok(0) => match self.action {
                    Action::Write => {
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

    fn serialize_response(&mut self, response: Response) -> io::Result<()> {
        response.serialize(&mut self.buf)
    }
}

struct Epoll {
    epoll_fd: epoll::Epoll,
    capacity: usize,
    conns: Vec<Connection>,
    id_pool: Vec<usize>,
}

impl Epoll {
    /// Creates a new Epoll instance.
    fn new(capacity: usize) -> Self {
        let epoll_fd = epoll::Epoll::new(epoll::EpollCreateFlags::empty()).unwrap();
        let conns = (0..capacity)
            .map(|_| Connection::new(None))
            .collect::<Vec<_>>();
        let id_pool = (0..capacity).collect::<Vec<_>>();

        Self {
            epoll_fd,
            capacity,
            conns,
            id_pool,
        }
    }

    /// Adds a connection and returns it's id.
    fn add(&mut self, stream: TcpStream) -> io::Result<()> {
        let id = self
            .id_pool
            .pop()
            .expect("cannot add a connection while connection pool is full.");

        // Add an entry to the epoll fd's interest list.
        let event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, id as u64);
        self.epoll_fd.add(&stream, event)?;

        let conn = &mut self.conns[id];
        conn.init(stream);

        Ok(())
    }

    /// Deletes a connection by id.
    fn delete(&mut self, id: usize) -> io::Result<()> {
        let conn = &mut self.conns[id];
        let stream = conn.stream.as_ref().expect("connection not in use.");

        self.epoll_fd.delete(stream)?;

        conn.reset(Action::Read);
        self.id_pool.push(id);

        Ok(())
    }

    fn modify(&mut self, id: usize, state: Action) -> io::Result<()> {
        let conn = &mut self.conns[id];
        let stream = conn.stream.as_ref().expect("connection not in use.");

        let event_flags = match state {
            Action::Read => epoll::EpollFlags::EPOLLIN,
            _ => epoll::EpollFlags::EPOLLOUT,
        };

        let mut event = epoll::EpollEvent::new(event_flags, id as u64);
        self.epoll_fd.modify(stream, &mut event)?;

        conn.reset(state);

        Ok(())
    }

    fn wait(&mut self, events: &mut [epoll::EpollEvent]) -> io::Result<usize> {
        let event_count = self.epoll_fd.wait(events, epoll::EpollTimeout::NONE)?;
        Ok(event_count)
    }

    /// Gets an immutable reference to a connection.
    fn get_ref(&self, id: usize) -> &Connection {
        &self.conns[id]
    }

    /// Gets a mutable reference to a connection.
    fn get_mut(&mut self, id: usize) -> &mut Connection {
        &mut self.conns[id]
    }

    /// Returns `true` if there are no connections in use.
    fn is_empty(&self) -> bool {
        self.id_pool.len() == self.capacity
    }

    /// Returns `true` if the connection pool is at capacity.
    fn is_full(&self) -> bool {
        self.id_pool.is_empty()
    }
}

struct EpollThread {
    /// The Epoll fd.
    epoll: Epoll,

    /// Reusable buffer of epoll events.
    events: Vec<epoll::EpollEvent>,

    /// The receiving side of a channel of connections.
    rx_conn: Receiver<TcpStream>,
}

impl EpollThread {
    /// Creates a new epoll thread.
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

    fn run(mut self) {
        loop {
            // We must have at least one connection
            if self.epoll.is_empty() {
                let stream = self.rx_conn.recv().unwrap();
                self.epoll.add(stream).unwrap();
            }

            // Keep accepting connections until we've reached the capacity or there
            // are no connections ready.
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

                        if e.kind() != io::ErrorKind::UnexpectedEof {
                            eprintln!("unexpected error: {e}");
                        }

                        self.epoll.delete(id).unwrap();
                    }
                    _ => match conn.action {
                        Action::Read => {
                            let response = conn.deserialize_request().unwrap().do_work();
                            conn.serialize_response(response).unwrap();
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
