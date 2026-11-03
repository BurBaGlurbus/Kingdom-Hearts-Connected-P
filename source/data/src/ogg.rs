//! Ogg pages and packets (RFC 3533) as the music SCDs carry them: page
//! headers, lacing, packet reassembly and granule positions. CRCs are not
//! checked (the exe's libogg sync would drop a bad page; the installed
//! data has none).
use std::io;

fn invalid(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_owned())
}

/// One page: its byte offset, granule position, flags and payload
/// segments.
#[derive(Clone, Debug, PartialEq)]
pub struct Page<'a> {
    pub offset: usize,
    pub granule: i64,
    /// Bit 0 continued packet, 1 first page, 2 last page.
    pub flags: u8,
    pub laces: &'a [u8],
    pub body: &'a [u8],
    /// Total page length in bytes.
    pub length: usize,
}

/// The page starting at `at`.
pub fn page(data: &[u8], at: usize) -> io::Result<Page<'_>> {
    let h = data
        .get(at..at + 27)
        .ok_or_else(|| invalid("truncated Ogg page header"))?;
    if &h[..4] != b"OggS" || h[4] != 0 {
        return Err(invalid("not an Ogg page"));
    }
    let segments = h[26] as usize;
    let laces = data
        .get(at + 27..at + 27 + segments)
        .ok_or_else(|| invalid("truncated Ogg lacing"))?;
    let size: usize = laces.iter().map(|&l| l as usize).sum();
    let start = at + 27 + segments;
    let body = data
        .get(start..start + size)
        .ok_or_else(|| invalid("truncated Ogg page body"))?;
    Ok(Page {
        offset: at,
        granule: i64::from_le_bytes(h[6..14].try_into().unwrap()),
        flags: h[5],
        laces,
        body,
        length: 27 + segments + size,
    })
}

/// A complete packet and the granule of the page where it ends (-1 when
/// another packet ends later on the same page).
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub data: Vec<u8>,
    pub granule: i64,
    /// Offset of the page the packet ends on.
    pub page: usize,
    /// The packet is the last one ending on its page.
    pub last_on_page: bool,
}

/// Reassembles packets from consecutive pages starting at `at`.
pub struct Packets<'a> {
    data: &'a [u8],
    at: usize,
    pending: Vec<u8>,
    queue: std::collections::VecDeque<Packet>,
}

impl<'a> Packets<'a> {
    pub fn new(data: &'a [u8], at: usize) -> Self {
        Self {
            data,
            at,
            pending: Vec::new(),
            queue: Default::default(),
        }
    }

    fn fill(&mut self) -> io::Result<bool> {
        if self.at >= self.data.len() {
            return Ok(false);
        }
        let p = page(self.data, self.at)?;
        let mut pos = 0;
        let mut ends = Vec::new();
        for (i, &l) in p.laces.iter().enumerate() {
            self.pending
                .extend_from_slice(&p.body[pos..pos + l as usize]);
            pos += l as usize;
            if l < 255 {
                ends.push((std::mem::take(&mut self.pending), i));
            }
        }
        let n = ends.len();
        for (k, (data, _)) in ends.into_iter().enumerate() {
            let last = k + 1 == n;
            self.queue.push_back(Packet {
                data,
                granule: if last { p.granule } else { -1 },
                page: p.offset,
                last_on_page: last,
            });
        }
        self.at += p.length;
        Ok(true)
    }

    pub fn next_packet(&mut self) -> io::Result<Option<Packet>> {
        while self.queue.is_empty() {
            if !self.fill()? {
                return Ok(None);
            }
        }
        Ok(self.queue.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_bytes(granule: i64, laces: &[u8], flags: u8) -> Vec<u8> {
        let mut b = b"OggS".to_vec();
        b.push(0);
        b.push(flags);
        b.extend(granule.to_le_bytes());
        b.extend([0; 12]);
        b.push(laces.len() as u8);
        b.extend(laces);
        let n: usize = laces.iter().map(|&l| l as usize).sum();
        b.extend((0..n).map(|i| i as u8));
        b
    }

    #[test]
    fn packets_span_pages_through_255_laces() {
        let mut data = page_bytes(-1, &[255], 0);
        data.extend(page_bytes(1000, &[10, 3], 1));
        let mut p = Packets::new(&data, 0);
        let a = p.next_packet().unwrap().unwrap();
        assert_eq!(a.data.len(), 265);
        assert_eq!(a.granule, -1);
        let b = p.next_packet().unwrap().unwrap();
        assert_eq!((b.data.len(), b.granule), (3, 1000));
        assert!(p.next_packet().unwrap().is_none());
    }
}
