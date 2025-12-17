use std::net::{TcpListener, TcpStream};

use crate::protocol::{self, Deserialize, Serialize};

use crate::utils::get_time;

fn run(addr: &str) {
    // Create our listener socket
    let mut listener = TcpListener::bind(addr).unwrap();

    // Accept connections
    for conn in listener.incoming() {
        handle_client(conn.unwrap());
    }
}

fn handle_client(mut stream: TcpStream) {
    // Deserialize and handle the request
    let request = protocol::Request::deserialize(&mut stream);
    request.work.do_work();

    // Serialize and send the response
    let response = protocol::Response {
        send_time: get_time(),
    };
    response.serialize(&mut stream);
}
