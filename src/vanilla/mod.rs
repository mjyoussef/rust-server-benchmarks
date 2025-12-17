use std::net::{TcpListener, TcpStream};

use crate::protocol;

fn run(addr: &str) {
    // Create our listener socket
    let mut listener = TcpListener::bind(addr).unwrap();

    // Accept connections
    for conn in listener.incoming() {
        handle_client(conn.unwrap());
    }
}

fn handle_client(stream: TcpStream) {
    //
}
