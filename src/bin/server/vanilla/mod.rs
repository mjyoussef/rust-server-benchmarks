use rust_server_benchmarks::get_time;
use rust_server_benchmarks::protocol::{Deserialize, Request, Response, Serialize};
use std::net::{SocketAddrV4, TcpListener, TcpStream};

pub fn run(addr: SocketAddrV4) {
    // Create our listener socket
    let listener = TcpListener::bind(addr).unwrap();
    println!("Server listening at {}", addr);

    // Accept connections
    for stream in listener.incoming() {
        std::thread::spawn(move || _handle_client(stream.unwrap()));
    }
}

fn _handle_client(mut stream: TcpStream) {
    stream.set_nodelay(true).unwrap();

    loop {
        // Deserialize and handle the request
        let request = Request::deserialize(&mut stream);
        request.work.do_work();

        // Serialize and send the response
        let response = Response {
            client_send_time: get_time(),
        };
        response.serialize(&mut stream);
    }
}
