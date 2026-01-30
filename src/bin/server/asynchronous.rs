use std::net::SocketAddrV4;

use rust_server_benchmarks::protocol::{REQUEST_SIZE, RESPONSE_SIZE, Request};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub fn run(addr: SocketAddrV4, n_threads: usize) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(n_threads)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        let listener = TcpListener::bind(addr).await.unwrap();
        println!("Server listening at {}", addr);

        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                handle_connection(stream).await;
            });
        }
    });
}

async fn handle_connection(mut stream: TcpStream) {
    // Reusable buffer for serializing/deserializing requests/responses
    let mut buf = vec![0u8; REQUEST_SIZE.max(RESPONSE_SIZE)];

    loop {
        // Receive request and do work
        if let Err(e) = stream.read_exact(&mut buf[..REQUEST_SIZE]).await {
            if e.kind() != io::ErrorKind::UnexpectedEof {
                eprintln!("{e}");
            }
            break;
        }

        let response = match Request::deserialize(&buf[..REQUEST_SIZE]) {
            Ok(request) => request.do_work(),
            Err(e) => {
                eprintln!("{e}");
                break;
            }
        };

        // Send response
        response.serialize(&mut buf[..RESPONSE_SIZE]);
        if let Err(e) = stream.write_all(&buf[..RESPONSE_SIZE]).await {
            eprintln!("{e}");
            break;
        }
    }
}
