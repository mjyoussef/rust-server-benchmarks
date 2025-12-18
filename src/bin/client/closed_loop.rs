use std::{
    net::{SocketAddrV4, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{Deserialize, LatencyRecord, Request, Response, Serialize, Work},
};

pub struct Config {
    /// The address of the server.
    pub addr: SocketAddrV4,

    /// The duration of time for which each client runs.
    pub runtime: Duration,

    /// The delay between when a client receives a response and sends the next request.
    pub delay: Duration,

    /// The work the server must do for the client.
    pub work: Work,

    /// The number of clients that are concurrently run.
    pub num_clients: usize,
}

impl Config {
    /// Runs the closed loop request generator and returns the latency records
    /// collected from all clients.
    pub fn run(self) -> Vec<LatencyRecord> {
        let cfg = Arc::new(self);

        let handles = (0..cfg.num_clients)
            .map(|_| {
                let cfg_clone = cfg.clone();
                std::thread::spawn(move || cfg_clone._run_client())
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .flatten()
            .collect::<Vec<_>>()
    }

    /// Runs an individual client.
    fn _run_client(&self) -> Vec<LatencyRecord> {
        let client_start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        // Connect to the server
        let mut stream = TcpStream::connect(self.addr).unwrap();
        stream.set_nodelay(true).unwrap();

        let mut latency_records = Vec::new();

        while client_start.elapsed() < self.runtime {
            let start = Instant::now();

            // Serialize and send request
            let req = Request {
                send_time: get_time(),
                work: self.work,
            };
            req.serialize(&mut stream);

            // Wait for the response and update our latency records
            let res = Response::deserialize(&mut stream);
            let lr = res.to_latency_record();
            latency_records.push(lr);

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

        latency_records
    }
}
