use std::time::{Duration, Instant};

use rust_server_benchmarks::{
    protocol::{Request, Response, Work},
    utils::get_time,
};

pub fn run_req_gen(duration: Duration, delay: Duration, work: Work) {
    let end = Instant::now() + duration;

    while Instant::now() < end {
        // Serialize and send request
        let req = Request {
            send_time: get_time(),
            work: work,
        };

        // Wait for the response
        // TODO

        // Wait
        // TODO
    }
}
