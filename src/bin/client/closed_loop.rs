use std::{
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{LatencyRecord, REQUEST_SIZE, RESPONSE_SIZE, Request, Response, Work},
};

pub struct Config {
    /// The address of the server.
    pub addr: SocketAddrV4,

    /// The duration of time for which each client runs.
    pub runtime: Duration,

    /// The work the server must do for the client.
    pub work: Work,

    /// The number of clients that are concurrently run.
    pub n_clients: usize,
}

impl Config {
    /// Runs the closed loop request generator and returns the latency records
    /// collected from all clients.
    pub fn run(self) -> Vec<LatencyRecord> {
        let cfg = Arc::new(self);

        let handles = (0..cfg.n_clients)
            .map(|_| {
                let cfg_clone = cfg.clone();
                std::thread::spawn(move || cfg_clone.run_client())
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .flatten()
            .collect::<Vec<_>>()
    }

    /// Runs an individual client.
    fn run_client(&self) -> Vec<LatencyRecord> {
        // Buffer for serialization/deserialization
        let mut buf = vec![0u8; REQUEST_SIZE.max(RESPONSE_SIZE)];

        let client_start = Instant::now();

        // Connect to the server
        let mut stream = TcpStream::connect(self.addr).unwrap();
        stream.set_nodelay(true).unwrap();

        let mut latency_records = Vec::new();

        while client_start.elapsed() < self.runtime {
            // Serialize and send request
            let req = Request {
                send_time: get_time(),
                work: self.work,
            };
            req.serialize(&mut buf[0..REQUEST_SIZE]);
            stream.write_all(&buf[0..REQUEST_SIZE]).unwrap();

            // Wait for the response and update the latency records
            stream.read_exact(&mut buf[0..RESPONSE_SIZE]).unwrap();
            let response = Response::deserialize(&buf[0..RESPONSE_SIZE]).unwrap();
            let lr = response.to_latency_record();
            latency_records.push(lr);
        }

        latency_records
    }
}
