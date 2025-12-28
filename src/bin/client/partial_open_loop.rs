use std::{
    net::{SocketAddrV4, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{Deserialize, LatencyRecord, Request, Response, Serialize, Work},
};

use crossbeam_channel::{Receiver, SendError, Sender, unbounded};

#[derive(Copy, Clone)]
pub struct Config {
    /// The address of the server.
    pub addr: SocketAddrV4,

    /// The duration of time for which the experiment is run.
    pub runtime: Duration,

    /// The delay between when a client receives a response and sends the next request.
    pub delay: Duration,

    /// The work the server must do for the client.
    pub work: Work,

    /// The maximum number of concurrent connections.
    pub max_connections: usize,
}

impl Config {
    pub fn run(self) {
        let start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        loop {
            let iter_start = Instant::now();

            // Start a new client
        }

        todo!()
    }

    /// Runs a single client of closed loop request generator. It returns the number of requests
    /// sent and the latency records received.
    fn _run_client(self: Arc<Self>) -> (JoinHandle<usize>, JoinHandle<Vec<LatencyRecord>>) {
        let stream = TcpStream::connect(self.addr).unwrap();
        stream.set_nodelay(true).unwrap();

        let done = Arc::new(AtomicBool::new(false));

        // Start the receiver (note: it is important to start the receiver first since spawning a
        // thread has substantial overhead and this can skew the latencies.
        let cfg_clone = self.clone();
        let stream_clone = stream.try_clone().unwrap();
        let done_clone = done.clone();
        let receiver =
            std::thread::spawn(move || cfg_clone._run_receiver(stream_clone, done_clone));

        // Start the sender
        let sender = std::thread::spawn(move || self._run_sender(stream, done));

        (sender, receiver)
    }

    /// Sends requests to the server.
    fn _run_sender(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> usize {
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
            req.serialize(&mut stream).unwrap();

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
    fn _run_receiver(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> Vec<LatencyRecord> {
        let mut lrs = Vec::new();

        while !done.load(Ordering::SeqCst) {
            let response = Response::deserialize(&mut stream).unwrap();
            let lr = response.to_latency_record();
            lrs.push(lr);
        }

        lrs
    }
}

struct ThreadPool<F> {
    /// The maximum number of threads that can be spawned.
    capacity: usize,

    /// Number of threads in the threadpool.
    size: usize,

    /// Number of spawned threads that are ready to execute a task.
    ready: Arc<AtomicU64>,

    /// Receiving side of the tasks channel.
    rx: Receiver<F>,

    /// Sending side of the tasks channel.
    tx: Sender<F>,
}

impl<F: FnOnce() + Send + 'static> ThreadPool<F> {
    fn new(capacity: usize) -> Self {
        let (tx, rx) = unbounded();
        Self {
            capacity,
            size: 0,
            ready: Arc::new(AtomicU64::new(0)),
            rx,
            tx,
        }
    }

    fn execute(&mut self, f: F) -> Result<(), SendError<F>> {
        // If all threads are busy and we aren't at capacity, spawn another thread
        if self.ready.load(Ordering::SeqCst) == 0 && self.size < self.capacity {
            self.size += 1;
            self.ready.fetch_add(1, Ordering::SeqCst);

            let ready = self.ready.clone();
            let rx = self.rx.clone();

            std::thread::spawn(move || {
                for f in rx {
                    ready.fetch_sub(1, Ordering::SeqCst);
                    f();
                    ready.fetch_add(1, Ordering::SeqCst);
                }
            });
        }

        // Send `f` down the channel.
        self.tx.send(f)?;
        Ok(())
    }
}
