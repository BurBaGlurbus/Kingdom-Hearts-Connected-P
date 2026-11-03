//! `khc_relay [ADDRESS:PORT]`: the relay on one UDP socket. It keeps no logs of who
//! connects. Develop and test on loopback: where it runs for real is the owner's to
//! decide (KHC-018).

use relay::Relay;
use std::hash::{BuildHasher, Hasher};
use std::net::{SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const DEFAULT_ADDRESS: &str = "127.0.0.1:47621";
/// The largest datagram read.
const DATAGRAM: usize = 2048;

fn main() -> ExitCode {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let address: SocketAddr = match address.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("usage: khc_relay [ADDRESS:PORT]: {address}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let socket = match UdpSocket::bind(address) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not bind {address}: {e}");
            return ExitCode::FAILURE;
        }
    };
    // A second now and then, so rooms expire with no traffic too.
    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
    let seed = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    let mut relay = Relay::new(seed);
    let start = Instant::now();
    println!("KHC relay on {address}");
    let mut buffer = [0u8; DATAGRAM];
    loop {
        let now = start.elapsed();
        match socket.recv_from(&mut buffer) {
            Ok((n, from)) => {
                for (to, bytes) in relay.handle(now, from, &buffer[..n]) {
                    let _ = socket.send_to(&bytes, to);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                let _ = relay.handle(now, SocketAddr::from(([0, 0, 0, 0], 0)), &[]);
            }
            Err(e) => {
                eprintln!("relay stopped: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
}
