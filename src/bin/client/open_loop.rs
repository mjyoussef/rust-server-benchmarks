use std::{
    net::{SocketAddrV4, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{Deserialize, LatencyRecord, Request, Response, Serialize, Work},
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
}

impl Config {
    /// Runs the closed loop request generator and returns the number of requests sent
    /// and the latency records.
    pub fn run(self) -> (usize, Vec<LatencyRecord>) {
        let cfg = Arc::new(self);

        let stream = TcpStream::connect(cfg.addr).unwrap();
        let done = Arc::new(AtomicBool::new(false));

        // Start the sender
        let sender = {
            let cfg_clone = cfg.clone();
            let stream_clone = stream.try_clone().unwrap();
            let done_clone = done.clone();
            std::thread::spawn(move || cfg_clone._run_sender(stream_clone, done_clone))
        };

        // Star the receiver
        let receiver = std::thread::spawn(move || cfg._run_receiver(stream, done));

        (sender.join().unwrap(), receiver.join().unwrap())
    }

    /// Sends requests to the server.
    fn _run_sender(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> usize {
        let client_start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        let mut requests_sent = 0;

        while client_start.elapsed() < self.runtime {
            let start = Instant::now();

            // Serialize and send request
            let req = Request {
                send_time: get_time(),
                work: self.work,
            };
            req.serialize(&mut stream);

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

        done.store(true, Ordering::SeqCst);

        requests_sent
    }

    /// Receives responses from the server.
    fn _run_receiver(&self, mut stream: TcpStream, done: Arc<AtomicBool>) -> Vec<LatencyRecord> {
        let mut latency_records = Vec::new();

        while !done.load(Ordering::SeqCst) {
            let res = Response::deserialize(&mut stream);
            let latency_record = res.to_latency_record();
            latency_records.push(latency_record);
        }

        latency_records
    }
}
