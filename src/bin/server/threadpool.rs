use crossbeam_channel::{SendError, Sender};
use rust_server_benchmarks::protocol::{Deserialize, Request, Serialize};
use std::io::ErrorKind;
use std::net::{SocketAddrV4, TcpListener, TcpStream};

pub fn run(addr: SocketAddrV4, tp_size: usize) {
    // Create our listener socket
    let listener = TcpListener::bind(addr).unwrap();

    // Start the threadpool
    let tp = ThreadPool::spawn(tp_size);

    println!("Server listening at {}", addr);

    // Accept connections
    for stream in listener.incoming() {
        tp.execute(move || _handle_client(stream.unwrap())).unwrap();
    }
}

fn _handle_client(mut stream: TcpStream) {
    stream.set_nodelay(true).unwrap();

    loop {
        // Deserialize and handle the request
        let response = match Request::deserialize(&mut stream) {
            Ok(request) => request.do_work(),
            Err(e) => {
                if e.kind() != ErrorKind::UnexpectedEof {
                    eprintln!("{e}");
                }

                break;
            }
        };

        // Serialize and send the response
        if let Err(e) = response.serialize(&mut stream) {
            eprintln!("{e}");
        }
    }
}

struct ThreadPool<F> {
    tx: Sender<F>,
}

impl<F: FnOnce() + Send + 'static> ThreadPool<F> {
    fn spawn(size: usize) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<F>();

        for _ in 0..size {
            let rx_clone = rx.clone();
            std::thread::spawn(|| {
                for f in rx_clone {
                    f();
                }
            });
        }

        Self { tx }
    }

    fn execute(&self, f: F) -> Result<(), SendError<F>> {
        self.tx.send(f)?;
        Ok(())
    }
}
