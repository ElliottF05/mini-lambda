use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let host = "httpbin.org";
    let path = "/get";

    let mut stream = TcpStream::connect((host, 80)).unwrap_or_else(|e| {
        eprint!("connection failed: {e}");
        std::process::exit(1);
    });

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );

    stream.write_all(request.as_bytes()).unwrap_or_else(|e| {
        eprint!("failed to send request: {e}");
        std::process::exit(1);
    });

    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap_or_else(|e| {
        eprint!("failed to read response: {e}");
        std::process::exit(1);
    });

    print!("{response}");
}