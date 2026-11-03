//! Area archives (`ard/<lang>/<world><nn>.ard`): the BAR of an area's spawn
//! scripts (type 13 `map` / `btl` / `evt`), spawn sets (type 12), world
//! scripts (type 3) and events (type 22).
//!
//! Layouts follow the exe's readers: script blocks FUN_1403a23e0 and
//! commands FUN_1403a24c0; spawn sets FUN_1403a4e80 / FUN_1403fdce0;
//! entities FUN_1403fe590 / FUN_1403ff550; activators FUN_140421010.
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct ArdError(pub &'static str);
impl fmt::Display for ArdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ArdError {}
type Result<T> = std::result::Result<T, ArdError>;

fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(ArdError("ard read past end"))
}
fn i16_at(d: &[u8], o: usize) -> Result<i16> {
    u16_at(d, o).map(|v| v as i16)
}
fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(ArdError("ard read past end"))
}
fn f32x3(d: &[u8], o: usize) -> Result<[f32; 3]> {
    Ok([
        f32::from_bits(u32_at(d, o)?),
        f32::from_bits(u32_at(d, o + 4)?),
        f32::from_bits(u32_at(d, o + 8)?),
    ])
}

/// A four-character BAR name as the exe packs it (first character in the
/// low byte).
pub fn tag(name: &str) -> u32 {
    let mut v = 0;
    for (i, b) in name.bytes().take(4).enumerate() {
        v |= (b as u32) << (8 * i);
    }
    v
}

/// One script command: `{u16 op, i16 argc}` and `argc` words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub op: u16,
    /// The raw argument words; `argc` of them.
    pub args: Vec<u32>,
}

impl Command {
    /// Parses the command at `at`; returns it and the offset of the next
    /// one (`at + (argc + 1) * 4`).
    pub fn read(d: &[u8], at: usize) -> Result<(Self, usize)> {
        let op = u16_at(d, at)?;
        let argc = i16_at(d, at + 2)?;
        let count = usize::try_from(argc).map_err(|_| ArdError("negative script argc"))?;
        let args = (0..count)
            .map(|i| u32_at(d, at + 4 + i * 4))
            .collect::<Result<_>>()?;
        Ok((Self { op, args }, at + (count + 1) * 4))
    }

    /// Nested commands of an `IfEntrance` (op 0x1c): they follow the
    /// entrance word inside this command's arguments.
    pub fn nested(&self) -> Result<Vec<Command>> {
        let bytes: Vec<u8> = self
            .args
            .iter()
            .skip(1)
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let (c, next) = Command::read(&bytes, at)?;
            out.push(c);
            at = next;
        }
        Ok(out)
    }
}

/// The commands of `program` in a type-13 script group (FUN_1403a23e0):
/// blocks `{i16 programId, i16 blockSize}` up to programId −1; the first
/// block whose id matches runs. `None` when no block matches.
pub fn program(script: &[u8], program: i16) -> Result<Option<Vec<Command>>> {
    let mut at = 0;
    loop {
        let id = i16_at(script, at)?;
        if id == -1 {
            return Ok(None);
        }
        let size = i16_at(script, at + 2)?;
        let size = usize::try_from(size).map_err(|_| ArdError("negative block size"))?;
        if size < 4 {
            return Err(ArdError("script block too small"));
        }
        let end = at + size;
        if id == program {
            let mut out = Vec::new();
            let mut cursor = at + 4;
            while cursor < end {
                let (c, next) = Command::read(script, cursor)?;
                out.push(c);
                cursor = next;
            }
            return Ok(Some(out));
        }
        at = end;
    }
}

/// Every program id in a script group, in file order.
pub fn program_ids(script: &[u8]) -> Result<Vec<i16>> {
    let mut at = 0;
    let mut out = Vec::new();
    loop {
        let id = i16_at(script, at)?;
        if id == -1 {
            return Ok(out);
        }
        out.push(id);
        let size = usize::try_from(i16_at(script, at + 2)?)
            .map_err(|_| ArdError("negative block size"))?;
        if size < 4 {
            return Err(ArdError("script block too small"));
        }
        at += size;
    }
}

/// A spawn-set entity (0x40 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// 00objentry id (566 PLAYER, 567/568 the friends).
    pub object: u32,
    /// Engine coordinates (Y down).
    pub position: [f32; 3],
    /// Radians; actors use only Y.
    pub rotation: [f32; 3],
    /// 0 always, 1 only at entrance `spawn_arg`, 2 counted/respawn, 3 always.
    pub spawn_type: u8,
    pub spawn_arg: u8,
    pub serial: u16,
    pub arg1: u32,
    pub arg2: u32,
    /// Libretto (talk) id.
    pub talk: u16,
    pub respawn_delay: u16,
    pub reaction: u16,
    pub radius: u16,
    pub required_count: u8,
    pub raw: [u8; 0x40],
}

/// A zone (event activator, 0x40 bytes) as FUN_140421010 reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Activator {
    /// 0 box, 1 cylinder, 2 infinite.
    pub shape: u16,
    pub option: u16,
    pub center: [f32; 3],
    pub extent: [f32; 3],
    pub rotation: [f32; 3],
    pub flags: u32,
    /// Type-8 points: 1-based background groups switched on / off.
    pub group_on: u8,
    pub group_off: u8,
    pub raw: [u8; 0x40],
}

#[derive(Clone, Debug, PartialEq)]
pub struct WalkPath {
    pub header: [u8; 8],
    pub points: Vec<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal {
    pub id: u16,
    pub arg: u16,
    pub kind: u8,
    pub raw: [u8; 8],
}

/// A spawn point: a 0x2c-byte header and its tables.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPoint {
    pub kind: u8,
    pub flags: u8,
    pub id: u16,
    pub header: [u8; 0x2c],
    pub entities: Vec<Entity>,
    pub activators: Vec<Activator>,
    pub walk_paths: Vec<WalkPath>,
    pub return_params: Vec<[u8; 16]>,
    pub signals: Vec<Signal>,
}

impl SpawnPoint {
    /// Type 1: destination area byte (+0x1c).
    pub fn exit_area(&self) -> u8 {
        self.header[0x1c]
    }
    /// Type 1: destination entrance (+0x1d).
    pub fn exit_entrance(&self) -> u8 {
        self.header[0x1d]
    }
    /// Type 1: zone index argument (+0x1e).
    pub fn exit_zone_arg(&self) -> u8 {
        self.header[0x1e]
    }
    /// Types 3/4/7: the u32 at +0x1c.
    pub fn header_word(&self) -> u32 {
        u32::from_le_bytes(self.header[0x1c..0x20].try_into().unwrap())
    }
}

/// A type-12 spawn set: `{u32 version, u32 count}` and `count` points laid
/// end to end.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnSet {
    pub version: u32,
    pub points: Vec<SpawnPoint>,
}

pub fn spawn_set(d: &[u8]) -> Result<SpawnSet> {
    // Empty sets (size 0) occur (tt08 'z_70'); the exe's lookup returns no
    // data for them.
    if d.is_empty() {
        return Ok(SpawnSet {
            version: 0,
            points: Vec::new(),
        });
    }
    let version = u32_at(d, 0)?;
    let count = u32_at(d, 4)?;
    let mut at = 8;
    let mut points = Vec::new();
    for _ in 0..count {
        let header: [u8; 0x2c] = d
            .get(at..at + 0x2c)
            .ok_or(ArdError("spawn point header past end"))?
            .try_into()
            .unwrap();
        let n = |o| u16::from_le_bytes([header[o], header[o + 1]]) as usize;
        let (ne, na, nw, nr, ns) = (n(4), n(6), n(8), n(0xa), n(0xc));
        at += 0x2c;
        let mut entities = Vec::with_capacity(ne);
        for _ in 0..ne {
            let raw: [u8; 0x40] = d
                .get(at..at + 0x40)
                .ok_or(ArdError("entity past end"))?
                .try_into()
                .unwrap();
            entities.push(Entity {
                object: u32_at(&raw, 0)?,
                position: f32x3(&raw, 4)?,
                rotation: f32x3(&raw, 0x10)?,
                spawn_type: raw[0x1c],
                spawn_arg: raw[0x1d],
                serial: u16_at(&raw, 0x1e)?,
                arg1: u32_at(&raw, 0x20)?,
                arg2: u32_at(&raw, 0x24)?,
                talk: u16_at(&raw, 0x28)?,
                respawn_delay: u16_at(&raw, 0x2a)?,
                reaction: u16_at(&raw, 0x2c)?,
                radius: u16_at(&raw, 0x2e)?,
                required_count: raw[0x30],
                raw,
            });
            at += 0x40;
        }
        let mut activators = Vec::with_capacity(na);
        for _ in 0..na {
            let raw: [u8; 0x40] = d
                .get(at..at + 0x40)
                .ok_or(ArdError("activator past end"))?
                .try_into()
                .unwrap();
            activators.push(Activator {
                shape: u16_at(&raw, 0)?,
                option: u16_at(&raw, 2)?,
                center: f32x3(&raw, 4)?,
                extent: f32x3(&raw, 0x10)?,
                rotation: f32x3(&raw, 0x1c)?,
                flags: u32_at(&raw, 0x28)?,
                group_on: raw[0x2e],
                group_off: raw[0x2f],
                raw,
            });
            at += 0x40;
        }
        let mut walk_paths = Vec::with_capacity(nw);
        for _ in 0..nw {
            let header: [u8; 8] = d
                .get(at..at + 8)
                .ok_or(ArdError("walk path past end"))?
                .try_into()
                .unwrap();
            let count = u16::from_le_bytes([header[2], header[3]]) as usize;
            let points = (0..count)
                .map(|i| f32x3(d, at + 8 + i * 12))
                .collect::<Result<_>>()?;
            walk_paths.push(WalkPath { header, points });
            at += 8 + count * 12;
        }
        let mut return_params = Vec::with_capacity(nr);
        for _ in 0..nr {
            return_params.push(
                d.get(at..at + 16)
                    .ok_or(ArdError("return param past end"))?
                    .try_into()
                    .unwrap(),
            );
            at += 16;
        }
        let mut signals = Vec::with_capacity(ns);
        for _ in 0..ns {
            let raw: [u8; 8] = d
                .get(at..at + 8)
                .ok_or(ArdError("signal past end"))?
                .try_into()
                .unwrap();
            signals.push(Signal {
                id: u16::from_le_bytes([raw[0], raw[1]]),
                arg: u16::from_le_bytes([raw[2], raw[3]]),
                kind: raw[4],
                raw,
            });
            at += 8;
        }
        points.push(SpawnPoint {
            kind: header[0],
            flags: header[1],
            id: u16::from_le_bytes([header[2], header[3]]),
            header,
            entities,
            activators,
            walk_paths,
            return_params,
            signals,
        });
    }
    Ok(SpawnSet { version, points })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(op: u16, args: &[u32]) -> Vec<u8> {
        let mut v = op.to_le_bytes().to_vec();
        v.extend((args.len() as i16).to_le_bytes());
        for a in args {
            v.extend(a.to_le_bytes());
        }
        v
    }

    fn block(id: i16, cmds: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = cmds.concat();
        let mut v = id.to_le_bytes().to_vec();
        v.extend(((body.len() + 4) as i16).to_le_bytes());
        v.extend(body);
        v
    }

    #[test]
    fn first_matching_block_runs() {
        let mut s = block(3, &[cmd(1, &[0x27, 0])]);
        s.extend(block(
            4,
            &[cmd(0, &[tag("m_00"), tag("m_10")]), cmd(0xe, &[1])],
        ));
        s.extend(block(4, &[cmd(0xe, &[9])]));
        s.extend((-1i16).to_le_bytes());
        s.extend([0, 0]);
        assert_eq!(program_ids(&s).unwrap(), vec![3, 4, 4]);
        let p = program(&s, 4).unwrap().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].args, vec![tag("m_00"), tag("m_10")]);
        assert_eq!(program(&s, 7).unwrap(), None);
    }

    #[test]
    fn nested_if_entrance() {
        let inner = [cmd(0, &[tag("b_40")]), cmd(0xe, &[2])].concat();
        let mut words = vec![5u32];
        words.extend(
            inner
                .chunks(4)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap())),
        );
        let bytes = cmd(0x1c, &words);
        let (c, next) = Command::read(&bytes, 0).unwrap();
        assert_eq!(next, bytes.len());
        let n = c.nested().unwrap();
        assert_eq!(n[0].op, 0);
        assert_eq!(n[1].args, vec![2]);
    }

    #[test]
    fn spawn_point_tables() {
        let mut d = 2u32.to_le_bytes().to_vec();
        d.extend(1u32.to_le_bytes());
        let mut h = [0u8; 0x2c];
        h[0] = 1;
        h[2] = 40;
        h[4] = 1; // one entity
        h[6] = 1; // one activator
        h[8] = 1; // one walk path
        h[0xa] = 1; // one return param
        h[0xc] = 1; // one signal
        h[0x1c] = 6;
        h[0x1d] = 3;
        d.extend(h);
        let mut e = [0u8; 0x40];
        e[0..4].copy_from_slice(&566u32.to_le_bytes());
        e[4..8].copy_from_slice(&(-2151.1f32).to_le_bytes());
        e[0x14..0x18].copy_from_slice(&1.7802f32.to_le_bytes());
        e[0x1c] = 1;
        e[0x1d] = 2;
        e[0x28..0x2a].copy_from_slice(&0x46bu16.to_le_bytes());
        d.extend(e);
        let mut a = [0u8; 0x40];
        a[2] = 1;
        a[0x10..0x14].copy_from_slice(&100f32.to_le_bytes());
        a[0x2e] = 3;
        d.extend(a);
        d.extend([0, 0, 2, 0, 0, 0, 0, 0]);
        d.extend([0u8; 24]);
        d.extend([0x55u8; 16]);
        d.extend([0x4a, 0, 7, 0, 1, 0, 0, 0]);
        let s = spawn_set(&d).unwrap();
        let p = &s.points[0];
        assert_eq!(
            (p.kind, p.id, p.exit_area(), p.exit_entrance()),
            (1, 40, 6, 3)
        );
        assert_eq!(p.entities[0].object, 566);
        assert_eq!(p.entities[0].rotation[1], 1.7802);
        assert_eq!((p.entities[0].spawn_type, p.entities[0].spawn_arg), (1, 2));
        assert_eq!(p.entities[0].talk, 0x46b);
        assert_eq!((p.activators[0].option, p.activators[0].group_on), (1, 3));
        assert_eq!(p.walk_paths[0].points.len(), 2);
        assert_eq!(p.return_params[0], [0x55; 16]);
        assert_eq!(
            (p.signals[0].id, p.signals[0].arg, p.signals[0].kind),
            (0x4a, 7, 1)
        );
        assert!(spawn_set(&d[..d.len() - 1]).is_err());
    }
}
