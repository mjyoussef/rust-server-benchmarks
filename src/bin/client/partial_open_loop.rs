use std::{
    net::{SocketAddrV4, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rust_server_benchmarks::{
    get_time,
    protocol::{Deserialize, LatencyRecord, Request, Response, Serialize, Work},
};

use crossbeam_channel::{Receiver, unbounded};

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
    pub max_clients: usize,

    /// Number of requests each client sends.
    pub n_requests: usize,
}

impl Config {
    pub fn run(self) -> Vec<LatencyRecord> {
        let start = Instant::now();
        let mut excess_duration = Duration::from_micros(0);

        // Notifications for the threads to run
        let (tx, rx) = unbounded();

        // Tracks the number of threads that are ready
        let ready = Arc::new(AtomicUsize::new(0));

        // Notification for threads to stop
        let done = Arc::new(AtomicBool::new(false));

        let mut handles = Vec::new();

        while start.elapsed() < self.runtime {
            let iter_start = Instant::now();

            // Spawn another thread if we haven't hit capacity and no threads are ready
            if ready.load(Ordering::Acquire) == 0 && handles.len() < self.max_clients {
                handles.push(self.run_client(&rx, &ready, &done));
            }

            tx.send(()).unwrap();

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

        // Drop the sender so that receivers will only process the remaining notifications.
        // If there are lots of notifications, it's possible that workers threads may continue
        // sending requests well after the deadline, so we also need to send a notification.
        drop(tx);
        done.store(true, Ordering::Release);

        handles
            .into_iter()
            .flat_map(|v| v.join().unwrap())
            .collect()
    }

    fn run_client(
        self,
        rx: &Receiver<()>,
        ready: &Arc<AtomicUsize>,
        done: &Arc<AtomicBool>,
    ) -> JoinHandle<Vec<LatencyRecord>> {
        let rx = rx.clone();
        let ready = ready.clone();
        let done = done.clone();

        std::thread::spawn(move || {
            ready.fetch_add(1, Ordering::Release);
            let mut lrs = Vec::new();

            for _ in rx {
                ready.fetch_sub(1, Ordering::AcqRel);
                if done.load(Ordering::Acquire) {
                    break;
                }

                let mut stream = TcpStream::connect(self.addr).unwrap();
                stream.set_nodelay(true).unwrap();

                for _ in 0..self.n_requests {
                    let req = Request {
                        send_time: get_time(),
                        work: self.work,
                    };
                    req.serialize(&mut stream).unwrap();

                    let resp = Response::deserialize(&mut stream).unwrap();
                    lrs.push(resp.to_latency_record());
                }

                ready.fetch_add(1, Ordering::AcqRel);
            }

            lrs
        })
    }
}
