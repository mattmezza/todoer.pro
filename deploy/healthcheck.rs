//! Minimal Docker readiness probe; deliberately uses only the Rust standard library.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

fn ready() -> std::io::Result<bool> {
    let address: SocketAddr = "127.0.0.1:8092".parse().unwrap();
    let timeout = Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    // Read only the status prefix; the server can split it across TCP packets.
    let mut status = [0; 13];
    stream.read_exact(&mut status)?;
    Ok(&status == b"HTTP/1.1 200 " || &status == b"HTTP/1.0 200 ")
}

fn main() {
    std::process::exit(if ready().unwrap_or(false) { 0 } else { 1 });
}
