use std::{
    io::{self, Cursor, Read, Write},
    net::{SocketAddrV4, TcpListener, TcpStream},
};

use nix::sys::epoll::*;

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

enum ConnState {
    Read,
    Write,
}

struct Connection {
    /// The connection stream.
    stream: TcpStream,

    /// A reusable buffer for reading from and writing to the client.
    buf: Cursor<Vec<u8>>,

    /// The current index into the buffer for reading or writing.
    idx: usize,

    /// The action being performed on the connection.
    state: ConnState,
}

impl Connection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream: stream,
            buf: Cursor::new(vec![0u8; REQUEST_SIZE]),
            idx: 0,
            state: ConnState::Read,
        }
    }

    fn change(&mut self, stream: TcpStream, state: ConnState) {
        self.stream = stream;
        self.reset(state);
    }

    fn reset(&mut self, state: ConnState) {
        match state {
            ConnState::Read => {
                self.buf.get_mut().truncate(REQUEST_SIZE);
            }
            ConnState::Write => {
                self.buf.get_mut().truncate(RESPONSE_SIZE);
            }
        }
        self.buf.set_position(0);
        self.idx = 0;
        self.state = state;
    }

    fn copy_until_block(&mut self) -> io::Result<()> {
        let size = match self.state {
            ConnState::Read => REQUEST_SIZE,
            _ => RESPONSE_SIZE,
        };

        loop {
            let result = match self.state {
                ConnState::Read => self.stream.read(&mut self.buf.get_mut()[self.idx..]),
                _ => self.stream.write(&mut self.buf.get_mut()[self.idx..]),
            };

            match result {
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

struct EpollThread {
    /// The epoll fd.
    epoll: Epoll,

    /// The maximum number of concurrent connections.
    capacity: usize,

    /// Reusable buffer of connections.
    conns: Box<[Connection]>,

    /// Indices of available connections in `conns`.
    available_conns: Vec<usize>,

    /// Reusable buffer of epoll events.
    events: Box<[EpollEvent]>,

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
        todo!()
    }

    fn run(mut self) {
        loop {
            // We must have at least one connection
            if self.available_conns.len() == self.capacity {
                // Block until we have a connection.
                let stream = self.rx_conn.recv().unwrap();
                self._add_connection(stream).unwrap();
            }

            // Keep accepting connections until we've reached the capacity or there
            // are no connections ready.
            while !self.available_conns.is_empty() {
                match self.rx_conn.try_recv() {
                    Ok(stream) => self._add_connection(stream).unwrap(),
                    _ => break,
                }
            }

            let event_count = self
                .epoll
                .wait(&mut self.events, EpollTimeout::NONE)
                .unwrap();

            for i in 0..event_count {
                let event = self.events[i];
                self.events[i] = EpollEvent::empty();

                let conn_idx = event.data() as usize;
                let conn = &mut self.conns[conn_idx];

                match conn.state {
                    ConnState::Read => {
                        // If ConnState::Read, keep reading until we hit a WouldBlock, Success, EoF, or error. Once complete,
                        // deserialize the request, do work, serialize the response into `conn.buf` and prepare the
                        // connection for writing and modify the epoll instance. If an EoF or error, then delete the event.
                        // If there is an error, print the error.
                        // TODO
                    }
                    ConnState::Write => {
                        // If ConnState::Write, keep writing until we hit a WouldBlock, Success, or error. Once complete,
                        // prepare the connection for reading and modify the epoll instance. If there is an error, delete
                        // the event and print the error.
                        // TODO
                    }
                }
            }
        }
    }

    fn _add_connection(&mut self, stream: TcpStream) -> io::Result<()> {
        let idx = self.available_conns.pop().unwrap();

        // Add an entry to the epoll fd's interest list.
        let event = EpollEvent::new(EpollFlags::EPOLLIN, idx as u64);
        self.epoll.add(&stream, event)?;

        // Prepare the connection
        let conn = &mut self.conns[idx];
        conn.change(stream, ConnState::Read);

        Ok(())
    }
}
