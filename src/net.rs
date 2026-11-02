
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub fn is_online() -> bool {
    const TIMEOUT: Duration = Duration::from_secs(2);
    for host in ["www.youtube.com:443", "1.1.1.1:443"] {
        let Ok(addrs) = host.to_socket_addrs() else {
            continue;
        };
        for addr in addrs {
            if TcpStream::connect_timeout(&addr, TIMEOUT).is_ok() {
                return true;
            }
        }
    }
    false
}
