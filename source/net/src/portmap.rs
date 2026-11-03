//! Asking the home router to open the game's port (KHC-051): UPnP's Internet
//! Gateway Device (SSDP to find it, then SOAP over HTTP), else NAT-PMP
//! (RFC 6886). Both run on a thread of their own, as the answers take up to a
//! few seconds; the mapping is leased, renewed while the session lives and
//! removed when it ends.

use crate::direct::Mapping;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How long a mapping is asked for; it is renewed at half that.
const LEASE: Duration = Duration::from_secs(1800);
const SSDP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 1900);
const SSDP_WAIT: Duration = Duration::from_millis(1200);
const NAT_PMP_PORT: u16 = 5351;
const HTTP_WAIT: Duration = Duration::from_secs(2);
/// What the router's list shows next to the mapping.
const DESCRIPTION: &str = "Kingdom Hearts: Connected";

/// The router's mapping for one session's port, on its own thread.
pub struct Mapper {
    /// In a lock only so the mapper can be shared between threads.
    result: Mutex<Receiver<Result<Mapping, String>>>,
    /// Dropped with the mapper: the thread removes the mapping and ends.
    _stop: Sender<()>,
}

impl Mapper {
    /// Asks for `port` (UDP) outside to reach `port` on this PC.
    pub fn start(port: u16) -> Mapper {
        let (result_tx, result) = mpsc::channel();
        let (stop_tx, stop) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let found = map(port, LEASE);
            let held = found.as_ref().ok().cloned();
            let _ = result_tx.send(found.map(|(m, _)| m));
            let Some((mapping, way)) = held else { return };
            loop {
                match stop.recv_timeout(LEASE / 2) {
                    Err(RecvTimeoutError::Timeout) => {
                        let _ = way.add(port, mapping.external.port(), LEASE);
                    }
                    _ => {
                        way.remove(mapping.external.port());
                        return;
                    }
                }
            }
        });
        Mapper {
            result: Mutex::new(result),
            _stop: stop_tx,
        }
    }

    /// The router's answer, once (None while it is still being asked).
    pub fn poll(&self) -> Option<Result<Mapping, String>> {
        self.result.lock().ok()?.try_recv().ok()
    }
}

/// This PC's address on the network that leads out (no datagram is sent).
pub fn local_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(a) if !a.ip().is_unspecified() => Some(*a.ip()),
        _ => None,
    }
}

/// How a mapping was made, to renew and remove it the same way.
#[derive(Clone)]
enum Way {
    Upnp(Gateway),
    NatPmp(Ipv4Addr),
}

impl Way {
    fn add(&self, port: u16, external: u16, lease: Duration) -> Result<(), String> {
        match self {
            Way::Upnp(g) => g.add(port, external, lease),
            Way::NatPmp(router) => nat_pmp_map(*router, port, external, lease).map(|_| ()),
        }
    }

    fn remove(&self, external: u16) {
        match self {
            Way::Upnp(g) => {
                let _ = g.soap(
                    "DeletePortMapping",
                    &format!(
                        "<NewRemoteHost></NewRemoteHost><NewExternalPort>{external}</NewExternalPort>\
                         <NewProtocol>UDP</NewProtocol>"
                    ),
                );
            }
            Way::NatPmp(router) => {
                let _ = nat_pmp_map(*router, external, 0, Duration::ZERO);
            }
        }
    }
}

fn map(port: u16, lease: Duration) -> Result<(Mapping, Way), String> {
    let local = local_ip().ok_or("this PC has no network address")?;
    let (gateway, router) = match discover(local) {
        Some((location, router)) => (Gateway::open(&location).ok(), Some(router)),
        None => (None, None),
    };
    let mut why = Vec::new();
    if let Some(g) = gateway {
        match g.map(port, lease) {
            Ok(m) => return Ok((m, Way::Upnp(g))),
            Err(e) => why.push(format!("UPnP: {e}")),
        }
    } else {
        why.push("UPnP: no router answered".into());
    }
    // NAT-PMP goes to the router: the one that answered UPnP, else the usual
    // first address of this PC's network.
    let [a, b, c, _] = local.octets();
    let router = router.unwrap_or(Ipv4Addr::new(a, b, c, 1));
    match nat_pmp_map(router, port, port, lease) {
        Ok(external_port) => {
            let ip = nat_pmp_address(router)?;
            Ok((
                Mapping {
                    external: SocketAddrV4::new(ip, external_port),
                    method: "NAT-PMP",
                },
                Way::NatPmp(router),
            ))
        }
        Err(e) => {
            why.push(format!("NAT-PMP: {e}"));
            Err(why.join("; "))
        }
    }
}

/// The first gateway that answers SSDP: its description's URL and address.
pub fn discover(local: Ipv4Addr) -> Option<(String, Ipv4Addr)> {
    let socket = UdpSocket::bind((local, 0)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    for kind in [
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
        "urn:schemas-upnp-org:service:WANIPConnection:1",
    ] {
        let ask = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\n\
             MX: 1\r\nST: {kind}\r\n\r\n"
        );
        let _ = socket.send_to(ask.as_bytes(), SSDP);
    }
    let until = Instant::now() + SSDP_WAIT;
    let mut buffer = [0u8; 2048];
    while Instant::now() < until {
        let Ok((n, SocketAddr::V4(from))) = socket.recv_from(&mut buffer) else {
            continue;
        };
        let text = String::from_utf8_lossy(&buffer[..n]);
        if let Some(location) = header(&text, "location") {
            return Some((location.to_owned(), *from.ip()));
        }
    }
    None
}

fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// `http://host:port/path` into its address and path.
fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:80")
    };
    Some((host, format!("/{path}")))
}

/// One HTTP/1.1 exchange, closed after: the status and the body.
fn http(host: &str, request: &str) -> Result<(u16, String), String> {
    let addr: SocketAddr = host
        .parse()
        .map_err(|_| format!("{host}: not an address"))?;
    let mut stream = TcpStream::connect_timeout(&addr, HTTP_WAIT).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(HTTP_WAIT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(HTTP_WAIT))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let _ = stream.take(256 * 1024).read_to_end(&mut bytes);
    parse_http(&bytes).ok_or_else(|| "no HTTP answer".into())
}

fn parse_http(bytes: &[u8]) -> Option<(u16, String)> {
    let text = String::from_utf8_lossy(bytes);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    let chunked = header(head, "transfer-encoding").is_some_and(|v| v.contains("chunked"));
    Some((
        status,
        if chunked {
            unchunk(body)
        } else {
            body.to_owned()
        },
    ))
}

fn unchunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
        else {
            break;
        };
        if size == 0 || rest.len() < size {
            out.push_str(rest.get(..size.min(rest.len())).unwrap_or(""));
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

/// The text of the first `<tag>` (any namespace prefix) in `xml`.
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut at = 0;
    while let Some(open) = xml[at..].find('<').map(|i| at + i) {
        let close = open + xml[open..].find('>')?;
        let element = &xml[open + 1..close];
        let local = element.rsplit(':').next().unwrap_or(element);
        if local == name {
            let end = xml[close..].find("</").map(|i| close + i)?;
            return Some(xml[close + 1..end].trim());
        }
        at = close;
    }
    None
}

/// A router's WAN connection service, from its UPnP description.
#[derive(Clone)]
struct Gateway {
    host: String,
    control: String,
    service: String,
    /// This PC's address as the router sees it.
    local: Ipv4Addr,
}

impl Gateway {
    fn open(location: &str) -> Result<Gateway, String> {
        let (host, path) = split_url(location).ok_or("not an http location")?;
        let (status, body) = http(
            &host,
            &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"),
        )?;
        if status != 200 {
            return Err(format!("description: HTTP {status}"));
        }
        let (service, control) = wan_service(&body).ok_or("no WAN connection service")?;
        let (host, control) = if control.starts_with("http://") {
            split_url(&control).ok_or("bad control URL")?
        } else if control.starts_with('/') {
            (host, control)
        } else {
            (host, format!("/{control}"))
        };
        let addr: SocketAddr = host.parse().map_err(|_| "bad router address")?;
        let local = match TcpStream::connect_timeout(&addr, HTTP_WAIT)
            .and_then(|s| s.local_addr())
            .map_err(|e| e.to_string())?
        {
            SocketAddr::V4(a) => *a.ip(),
            SocketAddr::V6(_) => return Err("an IPv6 router".into()),
        };
        Ok(Gateway {
            host,
            control,
            service,
            local,
        })
    }

    fn soap(&self, action: &str, arguments: &str) -> Result<String, String> {
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
             <u:{action} xmlns:u=\"{service}\">{arguments}</u:{action}></s:Body></s:Envelope>",
            service = self.service
        );
        let request = format!(
            "POST {control} HTTP/1.1\r\nHost: {host}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\n\
             SOAPAction: \"{service}#{action}\"\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}",
            control = self.control,
            host = self.host,
            service = self.service,
            length = body.len()
        );
        let (status, reply) = http(&self.host, &request)?;
        if status == 200 {
            Ok(reply)
        } else {
            Err(match tag(&reply, "errorCode") {
                Some(code) => format!("error {code}"),
                None => format!("HTTP {status}"),
            })
        }
    }

    fn add(&self, port: u16, external: u16, lease: Duration) -> Result<(), String> {
        let ask = |lease: u64| {
            self.soap(
                "AddPortMapping",
                &format!(
                    "<NewRemoteHost></NewRemoteHost><NewExternalPort>{external}</NewExternalPort>\
                     <NewProtocol>UDP</NewProtocol><NewInternalPort>{port}</NewInternalPort>\
                     <NewInternalClient>{local}</NewInternalClient><NewEnabled>1</NewEnabled>\
                     <NewPortMappingDescription>{DESCRIPTION}</NewPortMappingDescription>\
                     <NewLeaseDuration>{lease}</NewLeaseDuration>",
                    local = self.local
                ),
            )
        };
        match ask(lease.as_secs()) {
            // 725: this router only keeps mappings without a lease.
            Err(e) if e == "error 725" => ask(0).map(|_| ()),
            other => other.map(|_| ()),
        }
    }

    fn external_ip(&self) -> Result<Ipv4Addr, String> {
        let reply = self.soap("GetExternalIPAddress", "")?;
        tag(&reply, "NewExternalIPAddress")
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| "no external address".into())
    }

    fn map(&self, port: u16, lease: Duration) -> Result<Mapping, String> {
        let ip = self.external_ip()?;
        // 718: another PC has that port; the next few are tried.
        let mut last = String::new();
        for external in (0..4).map(|k| port.wrapping_add(k)).filter(|p| *p > 1024) {
            match self.add(port, external, lease) {
                Ok(()) => {
                    return Ok(Mapping {
                        external: SocketAddrV4::new(ip, external),
                        method: "UPnP",
                    });
                }
                Err(e) if e == "error 718" => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }
}

/// The WAN IP (or PPP) connection's service type and control URL.
fn wan_service(description: &str) -> Option<(String, String)> {
    description.split("<service>").skip(1).find_map(|block| {
        let kind = tag(block, "serviceType")?;
        let wanted = kind.contains("WANIPConnection") || kind.contains("WANPPPConnection");
        wanted.then(|| Some((kind.to_owned(), tag(block, "controlURL")?.to_owned())))?
    })
}

fn nat_pmp(router: Ipv4Addr, ask: &[u8], answer: usize) -> Result<Vec<u8>, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
    socket
        .connect((router, NAT_PMP_PORT))
        .map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 64];
    // RFC 6886 starts at 250 ms and doubles; three tries are enough here.
    for wait in [250, 500, 1000] {
        let _ = socket.send(ask);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(wait)));
        if let Ok(n) = socket.recv(&mut buffer)
            && n >= answer
            && buffer[0] == 0
            && buffer[1] == ask[1] + 128
        {
            let result = u16::from_be_bytes([buffer[2], buffer[3]]);
            if result != 0 {
                return Err(format!("result {result}"));
            }
            return Ok(buffer[..n].to_vec());
        }
    }
    Err("no answer".into())
}

/// Maps UDP `port` to `external` (0: remove); the external port given.
fn nat_pmp_map(router: Ipv4Addr, port: u16, external: u16, lease: Duration) -> Result<u16, String> {
    let mut ask = vec![0, 1, 0, 0];
    ask.extend(port.to_be_bytes());
    ask.extend(external.to_be_bytes());
    ask.extend((lease.as_secs() as u32).to_be_bytes());
    let answer = nat_pmp(router, &ask, 16)?;
    Ok(u16::from_be_bytes([answer[10], answer[11]]))
}

/// The router's own public address, by NAT-PMP.
pub fn nat_pmp_address(router: Ipv4Addr) -> Result<Ipv4Addr, String> {
    let answer = nat_pmp(router, &[0, 0], 12)?;
    Ok(Ipv4Addr::new(answer[8], answer[9], answer[10], answer[11]))
}

/// [Test] What this PC's router says when only asked (nothing is mapped):
/// UPnP's external address and NAT-PMP's.
pub fn probe() -> Vec<String> {
    let mut lines = Vec::new();
    let Some(local) = local_ip() else {
        return vec!["no network address".into()];
    };
    lines.push(format!("local {local}"));
    match discover(local) {
        Some((location, router)) => {
            lines.push(format!("SSDP answer from {router}"));
            match Gateway::open(&location).and_then(|g| g.external_ip()) {
                Ok(ip) => lines.push(format!("UPnP external {ip}")),
                Err(e) => lines.push(format!("UPnP: {e}")),
            }
        }
        None => lines.push("SSDP: no answer".into()),
    }
    let [a, b, c, _] = local.octets();
    match nat_pmp_address(Ipv4Addr::new(a, b, c, 1)) {
        Ok(ip) => lines.push(format!("NAT-PMP external {ip}")),
        Err(e) => lines.push(format!("NAT-PMP: {e}")),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_router_description_gives_its_wan_service() {
        let xml = "<root><device><serviceList>\
            <service><serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType>\
            <controlURL>/l3f</controlURL></service>\
            <service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>\
            <serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId>\
            <controlURL>/ctl/IPConn</controlURL></service>\
            </serviceList></device></root>";
        assert_eq!(
            wan_service(xml),
            Some((
                "urn:schemas-upnp-org:service:WANIPConnection:1".into(),
                "/ctl/IPConn".into()
            ))
        );
        assert_eq!(wan_service("<root/>"), None);
    }

    #[test]
    fn soap_answers_and_http_are_read() {
        let reply = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
            1a\r\n<s:Envelope><u:GetExtern\r\n\
            3c\r\nalIPAddressResponse><NewExternalIPAddress>203.0.113.9</NewExt\r\n\
            30\r\nernalIPAddress></u:GetExternalIPAddressResponse>\r\n0\r\n\r\n";
        let (status, body) = parse_http(reply.as_bytes()).unwrap();
        assert_eq!(status, 200);
        assert_eq!(tag(&body, "NewExternalIPAddress"), Some("203.0.113.9"));
        let fault =
            "<s:Fault><detail><UPnPError><errorCode>718</errorCode></UPnPError></detail></s:Fault>";
        assert_eq!(tag(fault, "errorCode"), Some("718"));
        assert_eq!(
            header(
                "HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.1:5000/rootDesc.xml\r\n",
                "location"
            ),
            Some("http://192.168.1.1:5000/rootDesc.xml")
        );
        assert_eq!(
            split_url("http://192.168.1.1:5000/rootDesc.xml"),
            Some(("192.168.1.1:5000".into(), "/rootDesc.xml".into()))
        );
        assert_eq!(
            split_url("http://10.0.0.1"),
            Some(("10.0.0.1:80".into(), "/".into()))
        );
    }

    #[test]
    fn nat_pmp_talks_to_a_router_on_loopback() {
        // A NAT-PMP router of our own on loopback, answering as RFC 6886 says.
        let router = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = router.local_addr().unwrap().port();
        let serve = std::thread::spawn(move || {
            let mut buffer = [0u8; 64];
            for _ in 0..2 {
                let (n, from) = router.recv_from(&mut buffer).unwrap();
                let mut answer = vec![0, buffer[1] + 128, 0, 0, 0, 0, 0, 9];
                if buffer[1] == 0 {
                    answer.extend([203, 0, 113, 5]);
                } else {
                    answer.extend(&buffer[4..6]);
                    answer.extend(40000u16.to_be_bytes());
                    answer.extend(&buffer[8..n.min(12)]);
                }
                router.send_to(&answer, from).unwrap();
            }
        });
        let ask = |kind: &[u8], size| {
            let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            socket.connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.send(kind).unwrap();
            let mut b = [0u8; 64];
            let n = socket.recv(&mut b).unwrap();
            assert!(n >= size);
            b[..n].to_vec()
        };
        let address = ask(&[0, 0], 12);
        assert_eq!(&address[8..12], &[203, 0, 113, 5]);
        let mut map = vec![0, 1, 0, 0];
        map.extend(47620u16.to_be_bytes());
        map.extend(47620u16.to_be_bytes());
        map.extend(1800u32.to_be_bytes());
        let mapped = ask(&map, 16);
        assert_eq!(u16::from_be_bytes([mapped[10], mapped[11]]), 40000);
        serve.join().unwrap();
    }

    /// [Live] What this PC's router and two public STUN servers say; maps
    /// nothing. `cargo test -p net -- --ignored live --nocapture`.
    #[test]
    #[ignore]
    fn live_router_and_stun() {
        for line in probe() {
            println!("{line}");
        }
        let servers = crate::direct::resolve(
            crate::direct::STUN_SERVERS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        )
        .recv()
        .unwrap();
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        for server in servers {
            let _ = socket.send_to(&crate::direct::stun_request([3; 12]), server);
            let mut b = [0u8; 512];
            match socket.recv_from(&mut b) {
                Ok((n, _)) => println!("STUN {server}: {:?}", crate::direct::stun_mapped(&b[..n])),
                Err(e) => println!("STUN {server}: {e}"),
            }
        }
    }
}
