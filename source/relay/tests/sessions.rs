//! Whole sessions through a real relay on loopback, on real sockets. No game, no
//! game data: bytes only.

use net::{Event, RELIABLE, Route, Session, SessionConfig, UNRELIABLE};
use relay::Relay;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

struct Server {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn server() -> Server {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let addr = socket.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let mut relay = Relay::new(0x1234_5678);
        let start = Instant::now();
        let mut buffer = [0u8; 2048];
        while !stopping.load(Ordering::Relaxed) {
            if let Ok((n, from)) = socket.recv_from(&mut buffer) {
                for (to, bytes) in relay.handle(start.elapsed(), from, &buffer[..n]) {
                    let _ = socket.send_to(&bytes, to);
                }
            }
        }
    });
    Server {
        addr,
        stop,
        thread: Some(thread),
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn config(name: &str, timeout: Duration) -> SessionConfig {
    SessionConfig {
        name: name.into(),
        build: [1; 32],
        version: "0.1.0 test".into(),
        caps: Default::default(),
        timeout,
        player: [0; net::wire::PLAYER_ID],
    }
}

/// Steps until `done`, or panics at `limit`; how long it took.
fn pump(
    a: &mut Session,
    b: &mut Session,
    limit: Duration,
    mut done: impl FnMut(&mut Session, &mut Session) -> bool,
) -> Duration {
    let start = Instant::now();
    while start.elapsed() < limit {
        a.update();
        b.update();
        if done(a, b) {
            return start.elapsed();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("not done in {limit:?}");
}

/// A host with a room, and its code.
fn room(server: &Server, timeout: Duration) -> (Session, [u8; net::wire::CODE_LENGTH]) {
    let mut host =
        Session::host_room(Ipv4Addr::LOCALHOST, 0, server.addr, config("Host", timeout)).unwrap();
    let start = Instant::now();
    loop {
        host.update();
        if let Some(Event::RoomCode(code)) = host.next_event() {
            return (host, net::wire::parse_code(&code).unwrap());
        }
        assert!(start.elapsed() < Duration::from_secs(2), "no code");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn exchange(host: &mut Session, join: &mut Session) {
    join.send(RELIABLE, b"ready".to_vec());
    host.send(RELIABLE, b"go".to_vec());
    let mut got = (None, None);
    pump(host, join, Duration::from_secs(3), |h, j| {
        got.0 = got.0.take().or_else(|| h.receive(RELIABLE));
        got.1 = got.1.take().or_else(|| j.receive(RELIABLE));
        got.0.is_some() && got.1.is_some()
    });
    assert_eq!(got, (Some(b"ready".to_vec()), Some(b"go".to_vec())));
}

#[test]
fn a_room_code_finds_the_host_directly_within_three_seconds() {
    let relay = server();
    let (mut host, code) = room(&relay, Duration::from_secs(5));
    let mut join =
        Session::join_room(relay.addr, code, config("Joiner", Duration::from_secs(5))).unwrap();
    let took = pump(&mut host, &mut join, Duration::from_secs(5), |h, j| {
        h.is_connected() && j.is_connected()
    });
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert_eq!(join.route(), Route::Direct);
    assert_eq!(host.route(), Route::Direct);
    assert_eq!(join.peer().map(|p| p.name.as_str()), Some("Host"));
    assert_eq!(host.peer().map(|p| p.name.as_str()), Some("Joiner"));
    exchange(&mut host, &mut join);
}

#[test]
fn with_no_way_through_both_go_by_the_relay() {
    let relay = server();
    let (mut host, code) = room(&relay, Duration::from_secs(5));
    let mut join =
        Session::join_room(relay.addr, code, config("Joiner", Duration::from_secs(5))).unwrap();
    host.set_relay_only(true);
    join.set_relay_only(true);
    let took = pump(&mut host, &mut join, Duration::from_secs(5), |h, j| {
        h.is_connected() && j.is_connected()
    });
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert_eq!(join.route(), Route::Relay);
    assert_eq!(host.route(), Route::Relay);
    exchange(&mut host, &mut join);
    // The unreliable channel too, and a leaving host is heard.
    host.send(UNRELIABLE, b"state".to_vec());
    let mut state = None;
    pump(&mut host, &mut join, Duration::from_secs(2), |_, j| {
        state = j.receive(UNRELIABLE);
        state.is_some()
    });
    host.close();
    pump(&mut host, &mut join, Duration::from_secs(2), |_, j| {
        j.is_closed()
    });
    let events: Vec<_> = std::iter::from_fn(|| join.next_event()).collect();
    assert!(
        matches!(events.last(), Some(Event::Disconnected(r)) if r == "the host left"),
        "{events:?}"
    );
}

#[test]
fn a_wrong_code_is_refused_with_a_reason() {
    let relay = server();
    let mut host = Session::host(
        Ipv4Addr::LOCALHOST,
        0,
        config("Unused", Duration::from_secs(5)),
    )
    .unwrap();
    let mut join = Session::join_room(
        relay.addr,
        *b"ZZZZZ",
        config("Joiner", Duration::from_secs(5)),
    )
    .unwrap();
    pump(&mut host, &mut join, Duration::from_secs(2), |_, j| {
        j.is_closed()
    });
    assert_eq!(
        join.next_event(),
        Some(Event::Rejected("no room has that code".into()))
    );
}

#[test]
fn a_cut_over_the_relay_comes_back_to_the_seat() {
    let relay = server();
    let timeout = Duration::from_millis(300);
    let (mut host, code) = room(&relay, timeout);
    let mut join = Session::join_room(relay.addr, code, config("Joiner", timeout)).unwrap();
    host.set_relay_only(true);
    join.set_relay_only(true);
    pump(&mut host, &mut join, Duration::from_secs(5), |h, j| {
        h.is_connected() && j.is_connected()
    });
    while host.next_event().is_some() {}
    while join.next_event().is_some() {}
    let hold = Some(Duration::from_secs(10));
    host.set_seat_hold(hold);
    join.set_seat_hold(hold);
    join.cut_for(Duration::from_secs(1));
    pump(&mut host, &mut join, Duration::from_secs(1), |h, _| {
        h.seat_waiting()
    });
    let mut resumed = (false, false);
    pump(&mut host, &mut join, Duration::from_secs(6), |h, j| {
        resumed.0 |= matches!(h.next_event(), Some(Event::Resumed(_)));
        resumed.1 |= matches!(j.next_event(), Some(Event::Resumed(_)));
        resumed.0 && resumed.1
    });
    assert_eq!(join.route(), Route::Relay);
    exchange(&mut host, &mut join);
}
