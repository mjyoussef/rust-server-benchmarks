use std::{
    io,
    net::{SocketAddrV4, TcpListener, TcpStream},
};

use nix::sys::epoll::*;

use crossbeam_channel::{Receiver, unbounded};

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
    buf: Vec<u8>,

    /// The action being performed on the connection.
    state: ConnState,
}

impl Connection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream: stream,
            buf: Vec::new(),
            state: ConnState::Read,
        }
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

                // If ConnState::Read, keep reading until we hit a WouldBlock, Success, EoF, or error. Once complete,
                // deserialize the request, do work, serialize the response into `conn.buf` and prepare the
                // connection for writing and modify the epoll instance. If an EoF or error, then delete the event.
                // If there is an error, print the error.
                // TODO

                // If ConnState::Write, keep writing until we hit a WouldBlock, Success, or error. Once complete,
                // prepare the connection for reading and modify the epoll instance. If there is an error, delete
                // the event and print the error.
                // TODO
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
        conn.stream = stream;
        conn.state = ConnState::Read;

        Ok(())
    }
}
