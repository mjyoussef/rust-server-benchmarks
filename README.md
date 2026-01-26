# rust-server-benchmarks
This repository contains Rust implementations and benchmarks of TCP servers built using thread pools, `epoll`, `io_uring`, and async Rust (`tokio`). It also includes closed-loop, open-loop, and partially open-loop request generators to evaluate performance under different workloads.

## Notes

* Spawning a large # of threads in any of the request generators can cause connections to unexpectedly terminate. This isn't exactly a bug. It's just that the server isn't able to handle the load and quickly hits resource limits, so the kernel forcefully shuts down sockets.
* You might notice no performance gain (and likely worse results compared to a vanilla server) if you're using a single thread for any of the request generators. This is expected since the servers are optimized for handling concurrent connections (e.g., via batching, parallelism, etc).