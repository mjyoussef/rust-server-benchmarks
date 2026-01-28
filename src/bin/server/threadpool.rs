use crossbeam_channel::{Receiver, unbounded};
use rust_server_benchmarks::protocol::{REQUEST_SIZE, RESPONSE_SIZE, Request};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddrV4, TcpListener, TcpStream};

pub fn run(addr: SocketAddrV4, tp_size: usize) {
    // Create our listener socket
    let listener = TcpListener::bind(addr).unwrap();

    println!("Server listening at {}", addr);

    // Create the threadpool
    let (tx, rx) = unbounded();
    create_thread_pool(tp_size, &rx);

    // Accept connections
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        stream.set_nodelay(true).unwrap();
        tx.send(stream).unwrap();
    }
}

fn create_thread_pool(tp_size: usize, rx: &Receiver<TcpStream>) {
    for _ in 0..tp_size {
        let rx = rx.clone();
        std::thread::spawn(move || {
            // Reusable buffer for serializing/deserializing requests/responses
            let mut buf = vec![0u8; REQUEST_SIZE.max(RESPONSE_SIZE)];

            for mut stream in rx {
                loop {
                    // Receive request and do work
                    if let Err(e) = stream.read_exact(&mut buf[..REQUEST_SIZE]) {
                        eprintln!("{e}");
                        break;
                    }

                    let response = match Request::deserialize(&buf[..REQUEST_SIZE]) {
                        Ok(request) => request.do_work(),
                        Err(e) => {
                            if e.kind() != ErrorKind::UnexpectedEof {
                                eprintln!("{e}");
                            }

                            break;
                        }
                    };

                    // Send response
                    response.serialize(&mut buf[..RESPONSE_SIZE]);
                    if let Err(e) = stream.write_all(&buf[..RESPONSE_SIZE]) {
                        eprintln!("{e}");
                        break;
                    }
                }
            }
        });
    }
}
