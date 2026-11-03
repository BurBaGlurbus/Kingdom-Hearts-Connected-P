//! Room codes that need no server of ours (KHC-051): the code is the host's
//! public address, so a friend who types it knocks on the host directly.
//!
//! The host learns that address by asking free public STUN servers (RFC 5389)
//! from the game's own socket, and asks its router to open the port (UPnP or
//! NAT-PMP, `portmap.rs`). Two STUN answers with different ports mean a router
//! that gives each destination its own port (symmetric NAT): nobody can reach
//! that host by code, and the player is told so instead of given one.
//!
//! A code is twelve characters of the relay codes' alphabet: the 48 bits of
//! address and port, mixed so neighbouring addresses do not look alike, and an
//! 11-bit check, so a mistyped code is refused instead of sending a friend to a
//! stranger's address. The mixing is not secrecy: whoever has the code has the
//! host's address, as with any game played without a server between.

use crate::wire::CODE_ALPHABET;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};

/// A direct code's characters, without the dashes it is shown with.
pub const DIRECT_CODE_LENGTH: usize = 12;
/// The STUN servers asked when the game names none: two operators, so their
/// answers can show a router that changes port per destination.
pub const STUN_SERVERS: [&str; 3] = [
    "stun.l.google.com:19302",
    "stun.cloudflare.com:3478",
    "stun1.l.google.com:19302",
];

const MASK: u64 = (1 << 48) - 1;
const CHECK_BITS: u32 = 11;
const KEY: u64 = 0x4b48_4320_3531; // "KHC 51"
const MULTIPLIER: u64 = 0x5_deec_e66d;

/// `m`'s inverse modulo 2^64 (m odd), by Newton's iteration.
const fn inverse(m: u64) -> u64 {
    let mut x = m;
    let mut i = 0;
    while i < 6 {
        x = x.wrapping_mul(2u64.wrapping_sub(m.wrapping_mul(x)));
        i += 1;
    }
    x
}

fn mix(x: u64) -> u64 {
    let x = ((x ^ KEY).wrapping_mul(MULTIPLIER)) & MASK;
    x ^ (x >> 24)
}

fn unmix(x: u64) -> u64 {
    let x = x ^ (x >> 24);
    (x.wrapping_mul(inverse(MULTIPLIER)) & MASK) ^ KEY
}

fn check(data: u64) -> u64 {
    data.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (64 - CHECK_BITS)
}

/// The code for `addr`, as shown: three groups of four.
pub fn encode_code(addr: SocketAddrV4) -> String {
    let data = (u64::from(u32::from(*addr.ip())) << 16) | u64::from(addr.port());
    let mut value = (mix(data) << CHECK_BITS) | check(data);
    let base = CODE_ALPHABET.len() as u64;
    let mut digits = [0u8; DIRECT_CODE_LENGTH];
    for d in digits.iter_mut().rev() {
        *d = CODE_ALPHABET[(value % base) as usize];
        value /= base;
    }
    let text = String::from_utf8_lossy(&digits).into_owned();
    format!("{}-{}-{}", &text[..4], &text[4..8], &text[8..])
}

/// The address in a code as a player types it: any case, dashes and spaces
/// ignored. None when it is not a direct code or its check fails.
pub fn parse_code(text: &str) -> Option<SocketAddrV4> {
    let text: Vec<u8> = text
        .bytes()
        .filter(|c| !matches!(c, b'-' | b' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if text.len() != DIRECT_CODE_LENGTH {
        return None;
    }
    let base = CODE_ALPHABET.len() as u64;
    let mut value: u64 = 0;
    for c in text {
        let digit = CODE_ALPHABET.iter().position(|a| *a == c)? as u64;
        value = value.checked_mul(base)?.checked_add(digit)?;
    }
    if value >> (48 + CHECK_BITS) != 0 {
        return None;
    }
    let data = unmix(value >> CHECK_BITS);
    if check(data) != value & ((1 << CHECK_BITS) - 1) {
        return None;
    }
    let addr = SocketAddrV4::new(Ipv4Addr::from((data >> 16) as u32), data as u16);
    (addr.port() != 0 && !addr.ip().is_unspecified()).then_some(addr)
}

const STUN_COOKIE: u32 = 0x2112_a442;

/// A STUN Binding Request with this transaction id.
pub fn stun_request(transaction: [u8; 12]) -> [u8; 20] {
    let mut out = [0u8; 20];
    out[..2].copy_from_slice(&1u16.to_be_bytes());
    out[4..8].copy_from_slice(&STUN_COOKIE.to_be_bytes());
    out[8..].copy_from_slice(&transaction);
    out
}

/// A STUN message's transaction id (anything shaped like one).
pub fn stun_transaction(bytes: &[u8]) -> Option<[u8; 12]> {
    let shaped = bytes.len() >= 20
        && bytes[0] & 0xc0 == 0
        && bytes[4..8] == STUN_COOKIE.to_be_bytes()
        && usize::from(u16::from_be_bytes([bytes[2], bytes[3]])) + 20 <= bytes.len();
    shaped.then(|| bytes[8..20].try_into().unwrap_or_default())
}

/// The address a Binding success response says we were seen from.
pub fn stun_mapped(bytes: &[u8]) -> Option<SocketAddrV4> {
    stun_transaction(bytes)?;
    if u16::from_be_bytes([bytes[0], bytes[1]]) != 0x0101 {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
    let mut at = 20;
    let mut plain = None;
    while at + 4 <= 20 + length {
        let kind = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        let size = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        let value = bytes.get(at + 4..at + 4 + size)?;
        // IPv4 only: family 1, a port and four bytes.
        if size >= 8 && value[1] == 1 {
            let port = u16::from_be_bytes([value[2], value[3]]);
            let ip = u32::from_be_bytes([value[4], value[5], value[6], value[7]]);
            match kind {
                // XOR-MAPPED-ADDRESS, and its pre-RFC number.
                0x0020 | 0x8020 => {
                    return Some(SocketAddrV4::new(
                        Ipv4Addr::from(ip ^ STUN_COOKIE),
                        port ^ (STUN_COOKIE >> 16) as u16,
                    ));
                }
                0x0001 => plain = Some(SocketAddrV4::new(Ipv4Addr::from(ip), port)),
                _ => {}
            }
        }
        at += 4 + size.div_ceil(4) * 4;
    }
    plain
}

/// Looks the STUN servers' names up away from the game's thread.
pub fn resolve(names: Vec<String>) -> Receiver<Vec<SocketAddrV4>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut found: Vec<SocketAddrV4> = Vec::new();
        for name in names {
            let first = name.to_socket_addrs().ok().and_then(|mut a| {
                a.find_map(|a| match a {
                    SocketAddr::V4(v4) => Some(v4),
                    SocketAddr::V6(_) => None,
                })
            });
            if let Some(a) = first
                && !found.contains(&a)
            {
                found.push(a);
            }
        }
        let _ = tx.send(found);
    });
    rx
}

/// Where a router's own address is not reachable from the internet: private,
/// or a provider's shared space (CGNAT, RFC 6598).
pub fn is_inner(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || (a == 100 && (64..128).contains(&b))
}

/// What the router said when asked to open the port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub external: SocketAddrV4,
    /// "UPnP" or "NAT-PMP".
    pub method: &'static str,
}

/// The address a friend should knock on, with a line for the player when the
/// way in is uncertain; or why there is none.
pub fn choose(
    seen: &[SocketAddrV4],
    mapping: Option<&Mapping>,
    local: Option<Ipv4Addr>,
) -> Result<(SocketAddrV4, Option<String>), String> {
    let behind_another = |m: &Mapping| {
        is_inner(*m.external.ip()) || seen.first().is_some_and(|s| s.ip() != m.external.ip())
    };
    if let Some(m) = mapping
        && !behind_another(m)
    {
        return Ok((m.external, None));
    }
    let shared = mapping.is_some() || seen.first().is_some_and(|s| is_inner(*s.ip()));
    let Some(first) = seen.first() else {
        return Err(
            "No STUN server answered, so this PC's internet address is unknown. \
                    Check the connection, or play on the LAN."
                .into(),
        );
    };
    if seen.iter().any(|s| s != first) {
        return Err(format!(
            "Your {} gives every connection its own port, so nobody can reach this PC by \
             code. Let your friend host, or play on the LAN.",
            if shared {
                "internet provider (CGNAT)"
            } else {
                "router"
            }
        ));
    }
    if local == Some(*first.ip()) {
        return Ok((*first, None));
    }
    Ok((
        *first,
        Some(if shared {
            "Behind a second router or your provider's: a friend may not get in.".into()
        } else {
            "The router did not open the port: turn on UPnP if a friend can't get in.".into()
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> SocketAddrV4 {
        text.parse().unwrap()
    }

    #[test]
    fn codes_round_trip_and_look_unalike() {
        for addr in ["203.0.113.7:47620", "1.2.3.4:1", "255.255.255.254:65535"] {
            let code = encode_code(at(addr));
            assert_eq!(code.len(), DIRECT_CODE_LENGTH + 2);
            assert_eq!(parse_code(&code), Some(at(addr)), "{code}");
            assert_eq!(
                parse_code(&code.to_lowercase().replace('-', " ")),
                Some(at(addr))
            );
        }
        let a = encode_code(at("203.0.113.7:47620"));
        let b = encode_code(at("203.0.113.8:47620"));
        assert!(
            a.bytes().zip(b.bytes()).filter(|(x, y)| x != y).count() > 4,
            "{a} {b}"
        );
    }

    #[test]
    fn a_mistyped_code_is_refused() {
        let code = encode_code(at("198.51.100.20:47620")).replace('-', "");
        let mut refused = 0;
        let mut tried = 0;
        for i in 0..DIRECT_CODE_LENGTH {
            for c in CODE_ALPHABET {
                let mut typo = code.clone().into_bytes();
                if typo[i] == *c {
                    continue;
                }
                typo[i] = *c;
                tried += 1;
                if parse_code(std::str::from_utf8(&typo).unwrap()).is_none() {
                    refused += 1;
                }
            }
        }
        // An 11-bit check lets about one typo in 2048 through.
        assert!(refused * 100 >= tried * 99, "{refused} of {tried}");
        assert_eq!(parse_code("ABC234"), None);
        assert_eq!(parse_code("0000-0000-0000"), None);
    }

    #[test]
    fn stun_answers_are_read() {
        let tx = [7u8; 12];
        let request = stun_request(tx);
        assert_eq!(stun_transaction(&request), Some(tx));
        // A success response with XOR-MAPPED-ADDRESS 192.0.2.1:32853 (RFC 5769's
        // numbers) behind an attribute it does not know.
        let mut answer = vec![0x01, 0x01, 0, 20];
        answer.extend(STUN_COOKIE.to_be_bytes());
        answer.extend(tx);
        answer.extend([0x80, 0x22, 0, 3, b'a', b'b', b'c', 0]);
        answer.extend([0x00, 0x20, 0, 8, 0, 1, 0xa1, 0x47, 0xe1, 0x12, 0xa6, 0x43]);
        assert_eq!(stun_mapped(&answer), Some(at("192.0.2.1:32853")));
        assert_eq!(stun_mapped(&request), None);
        assert_eq!(stun_mapped(&answer[..30]), None);
        assert_eq!(stun_transaction(b"KH2D............................"), None);
    }

    #[test]
    fn the_address_is_chosen_with_what_the_router_said() {
        let public = at("203.0.113.7:50000");
        let mapped = Mapping {
            external: at("203.0.113.7:47620"),
            method: "UPnP",
        };
        // The router's mapping wins when it is the address the world sees.
        assert_eq!(
            choose(&[public], Some(&mapped), None),
            Ok((mapped.external, None))
        );
        assert_eq!(
            choose(&[], Some(&mapped), None),
            Ok((mapped.external, None))
        );
        // No mapping: STUN's address, with a line about UPnP.
        let (addr, note) = choose(&[public, public], None, None).unwrap();
        assert_eq!(addr, public);
        assert!(note.unwrap().contains("UPnP"));
        // A PC on a public address needs no line.
        assert_eq!(
            choose(&[public], None, Some(*public.ip())),
            Ok((public, None))
        );
        // A mapping on an inner router: STUN's address, behind another.
        let inner = Mapping {
            external: at("100.70.1.2:47620"),
            method: "NAT-PMP",
        };
        let (addr, note) = choose(&[public], Some(&inner), None).unwrap();
        assert_eq!(addr, public);
        assert!(note.unwrap().contains("second router"));
        // Two ports: no code.
        let other = at("203.0.113.7:50001");
        assert!(
            choose(&[public, other], None, None)
                .unwrap_err()
                .contains("router")
        );
        assert!(
            choose(&[public, other], Some(&inner), None)
                .unwrap_err()
                .contains("CGNAT")
        );
        assert!(choose(&[], None, None).unwrap_err().contains("STUN"));
    }
}
