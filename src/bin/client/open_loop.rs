use std::{
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{LatencyRecord, REQUEST_SIZE, RESPONSE_SIZE, Request, Response, Work},
};

pub struct Config {
    /// The address of the server.
    pub addr: SocketAddrV4,

    /// The duration of time for which the experiment is run.
    pub runtime: Duration,

    /// The delay between when a client receives a response and sends the next request.
    pub delay: Duration,

    /// The work the server must do for the client.
    pub work: Work,

    /// The number of clients that are concurrently run.
    pub n_clients: usize,
}

impl Config {
    pub fn run(self) -> (usize, Vec<LatencyRecord>) {
        let cfg = Arc::new(self);

        let handles: Vec<_> = (0..cfg.n_clients)
            .map(|_| {
                let cfg_clone = cfg.clone();
                cfg_clone.run_client()
            })
            .collect();

        let mut n_reqs = 0;
        let mut lrs = Vec::new();

        for handle in handles {
            n_reqs += handle.0.join().unwrap();
            lrs.append(&mut handle.1.join().unwrap());
        }

        (n_reqs, lrs)
    }

    /// Runs a single client of closed loop request generator. It returns the number of requests
    /// sent and the latency records received.
    fn run_client(self: Arc<Self>) -> (JoinHandle<usize>, JoinHandle<Vec<LatencyRecord>>) {
        let stream = TcpStream::connect(self.addr).unwrap();
        stream.set_nodelay(true).unwrap();

        let done = Arc::new(AtomicBool::new(false));

        // Start the receiver (note: it is important to start the receiver first since spawning a
        // thread has substantial overhead and this can skew the latencies.
        let cfg_clone = self.clone();
        let stream_clone = stream.try_clone().unwrap();
        let done_clone = done.clone();
        let receiver = std::thread::spawn(move || cfg_clone.run_receiver(stream_clone, done_clone));

        // Start the sender
        let sender = std::thread::spawn(move || self.run_sender(stream, done));

        (sender, receiver)
    }

    /// Sends requests to the server.
    fn run_sender(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> usize {
        // Buffer for serializing
        let mut buf = vec![0u8; REQUEST_SIZE];

        let client_start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        let mut requests_sent = 0;

        loop {
            let start = Instant::now();

            // We have to make sure there is an outstanding request before `done` is
            // true to avoid deadlocking the receiver when the last request has been sent.
            let is_last = client_start.elapsed() >= self.runtime;
            if is_last {
                done.store(true, Ordering::SeqCst);
            }

            // Serialize and send request
            let req = Request {
                send_time: get_time(),
                work: self.work,
            };
            req.serialize(&mut buf);
            stream.write_all(&buf).unwrap();

            if is_last {
                return requests_sent;
            }

            requests_sent += 1;

            // Factor in the excess time
            excess_duration += start.elapsed();
            let excess_delay = excess_duration.min(self.delay);
            let busy_wait_time = self.delay - excess_delay;
            excess_duration -= excess_delay;

            // Busy loop
            let busy_loop_start = Instant::now();
            while busy_loop_start.elapsed() < busy_wait_time {
                std::hint::spin_loop();
            }
        }
    }

    /// Receives responses from the server.
    fn run_receiver(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> Vec<LatencyRecord> {
        // Buffer for deserializing
        let mut buf = vec![0u8; RESPONSE_SIZE];

        let mut lrs = Vec::new();

        while !done.load(Ordering::SeqCst) {
            stream.read_exact(&mut buf).unwrap();
            let response = Response::deserialize(&mut buf).unwrap();
            let lr = response.to_latency_record();
            lrs.push(lr);
        }

        lrs
    }
}
