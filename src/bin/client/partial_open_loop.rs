use std::{
    net::{SocketAddrV4, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{Deserialize, LatencyRecord, Request, Response, Serialize, Work},
};

use crossbeam_channel::{Receiver, Sender, unbounded};

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

    /// The maximum number of client threads that can be running concurrently.
    pub max_threads: usize,

    /// Number of requests each client sends.
    pub num_requests: usize,
}

impl Config {
    pub fn run(self) -> Vec<LatencyRecord> {
        let start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        // Notifications for the threads run
        let (tx, rx) = unbounded();

        // Number of idle threads
        let ready = Arc::new(AtomicU64::new(0));

        let mut handles: Vec<JoinHandle<Vec<LatencyRecord>>> = Vec::new();

        while start.elapsed() < self.runtime {
            let iter_start = Instant::now();

            self._run_client(&tx, &rx, &ready, &mut handles);

            // Factor in the excess time
            excess_duration += iter_start.elapsed();
            let excess_delay = excess_duration.min(self.delay);
            let busy_wait_time = self.delay - excess_delay;
            excess_duration -= excess_delay;

            // Busy loop
            let busy_loop_start = Instant::now();
            while busy_loop_start.elapsed() < busy_wait_time {
                std::hint::spin_loop();
            }
        }

        // Drop the sender so that receivers will exit out of the receive loop.
        // Otherwise, we'll deadlock.
        drop(tx);

        handles
            .into_iter()
            .flat_map(|v| v.join().unwrap())
            .collect()
    }

    fn _run_client(
        self,
        tx: &Sender<()>,
        rx: &Receiver<()>,
        ready: &Arc<AtomicU64>,
        handles: &mut Vec<JoinHandle<Vec<LatencyRecord>>>,
    ) {
        // If all threads are busy and we haven't reached the threadpool capacity, spawn another thread.
        if ready.load(Ordering::SeqCst) == 0 && handles.len() < self.max_threads {
            let rx = rx.clone();
            let ready = ready.clone();
            let handle = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(self.addr).unwrap();
                let mut lrs = Vec::new();

                for _ in rx {
                    ready.fetch_sub(1, Ordering::SeqCst);
                    for _ in 0..self.num_requests {
                        let req = Request {
                            send_time: get_time(),
                            work: self.work,
                        };
                        req.serialize(&mut stream).unwrap();

                        let resp = Response::deserialize(&mut stream).unwrap();
                        lrs.push(resp.to_latency_record());
                    }
                    ready.fetch_add(1, Ordering::SeqCst);
                }

                lrs
            });

            handles.push(handle);
        }

        // Either way, send a notification.
        tx.send(()).unwrap();
    }
}
