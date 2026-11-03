//! 2LD layouts: a BAR holding an IMGZ (type 29, several IMGDs) and a LAYD
//! (type 28): props placing SEQD animation groups at positions and start
//! times, grouped into scenes (layout read by FUN_140194d60; field meaning
//! from FUN_140194550 / FUN_140194780 / FUN_1401948c0).
use crate::sprite_sequence::{self, Sequence};
use std::io;

fn invalid(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_owned())
}

fn i32_at(b: &[u8], at: usize) -> io::Result<i32> {
    b.get(at..at + 4)
        .map(|w| i32::from_le_bytes(w.try_into().unwrap()))
        .ok_or_else(|| invalid("layout truncated"))
}

fn u16_at(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2)
        .map(|w| u16::from_le_bytes([w[0], w[1]]))
        .ok_or_else(|| invalid("layout truncated"))
}

/// One placed sequence group: texture, SEQD, group, start frame, position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prop {
    pub texture: usize,
    pub sequence: usize,
    pub group: usize,
    pub show_at: i32,
    pub position: [i32; 2],
}

/// A scene: a run of props and an optional last frame (+8; 0 = none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub props: std::ops::Range<usize>,
    pub max_frame: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub props: Vec<Prop>,
    pub scenes: Vec<Scene>,
    pub sequences: Vec<Sequence>,
}

pub fn read_layd(bytes: &[u8]) -> io::Result<Layout> {
    if bytes.get(..4) != Some(b"LAYD") {
        return Err(invalid("not a LAYD"));
    }
    let count = |at: usize, limit: usize| -> io::Result<(usize, usize)> {
        let (n, o) = (i32_at(bytes, at)?, i32_at(bytes, at + 4)?);
        let (n, o) = (
            usize::try_from(n).map_err(|_| invalid("negative LAYD count"))?,
            usize::try_from(o).map_err(|_| invalid("negative LAYD offset"))?,
        );
        if n > limit {
            return Err(invalid("LAYD count outside inspection bounds"));
        }
        Ok((n, o))
    };
    let (prop_count, props_at) = count(8, 4096)?;
    let (scene_count, scenes_at) = count(16, 1024)?;
    let (seq_count, seqs_at) = count(24, 256)?;
    let mut sequences = Vec::with_capacity(seq_count);
    for k in 0..seq_count {
        let at = i32_at(bytes, seqs_at + 4 * k)?;
        let at = usize::try_from(at).map_err(|_| invalid("negative SEQD offset"))?;
        sequences.push(sprite_sequence::read(
            bytes
                .get(at..)
                .ok_or_else(|| invalid("SEQD outside LAYD"))?,
        )?);
    }
    let props = (0..prop_count)
        .map(|k| {
            let a = props_at + k * 0x18;
            let index = |o: usize| -> io::Result<usize> {
                usize::try_from(i32_at(bytes, a + o)?).map_err(|_| invalid("negative prop index"))
            };
            let prop = Prop {
                texture: index(0)?,
                sequence: index(4)?,
                group: index(8)?,
                show_at: i32_at(bytes, a + 12)?,
                position: [i32_at(bytes, a + 16)?, i32_at(bytes, a + 20)?],
            };
            let seq = sequences
                .get(prop.sequence)
                .ok_or_else(|| invalid("prop SEQD outside layout"))?;
            if prop.group >= seq.groups.len() {
                return Err(invalid("prop group outside SEQD"));
            }
            Ok(prop)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let scenes = (0..scene_count)
        .map(|k| {
            let a = scenes_at + k * 0x14;
            let first = u16_at(bytes, a)? as usize;
            let n = u16_at(bytes, a + 2)? as usize;
            if first + n > props.len() {
                return Err(invalid("scene props outside layout"));
            }
            Ok(Scene {
                props: first..first + n,
                max_frame: i32_at(bytes, a + 8)?,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Layout {
        props,
        scenes,
        sequences,
    })
}

/// IMGZ: count at +0xc, then {offset, size} pairs from +0x10, relative to
/// the IMGZ start; each is an IMGD.
pub fn read_imgz(bytes: &[u8]) -> io::Result<Vec<&[u8]>> {
    let n = i32_at(bytes, 12)?;
    if !(0..=256).contains(&n) {
        return Err(invalid("IMGZ count outside inspection bounds"));
    }
    (0..n as usize)
        .map(|k| {
            let o = i32_at(bytes, 16 + 8 * k)? as usize;
            let s = i32_at(bytes, 20 + 8 * k)? as usize;
            bytes
                .get(o..o + s)
                .ok_or_else(|| invalid("IMGD outside IMGZ"))
        })
        .collect()
}

/// FUN_140192c60: frames a group needs, the latest animation end (one less
/// for last-cut animations) plus one. Loop points are not consulted.
fn group_length(sequence: &Sequence, group: usize) -> i32 {
    let mut end = 0;
    for a in &sequence.animations[sequence.groups[group].animations.clone()] {
        if end < a.frames[1] {
            end = a.frames[1] - (a.flags & sprite_sequence::LAST_CUT != 0) as i32;
        }
    }
    end + 1
}

/// YI::LAYOUT playing one scene (FUN_140194550 start, FUN_140194780
/// update, FUN_140194c90 alive, FUN_1401948c0 draw).
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePlayer {
    pub scene: usize,
    pub time: f32,
    /// 1 = just set, 2 = playing, 3 = finished.
    pub state: u8,
    end: f32,
    looping: bool,
    players: Vec<sprite_sequence::Player>,
}

impl ScenePlayer {
    /// FUN_140194ff0 / FUN_140194550: one sequence player per prop; the
    /// end is the latest prop's `show_at + length - 1`, capped by the
    /// scene's own last frame; any prop group with play count 0 loops.
    pub fn new(layout: &Layout, scene: usize) -> Self {
        let s = &layout.scenes[scene];
        let props = &layout.props[s.props.clone()];
        let mut end = props
            .iter()
            .map(|p| p.show_at + group_length(&layout.sequences[p.sequence], p.group))
            .fold(0, i32::max)
            - 1;
        if s.max_frame != 0 && s.max_frame < end {
            end = s.max_frame;
        }
        Self {
            scene,
            time: 0.,
            state: 1,
            end: end as f32,
            looping: props
                .iter()
                .any(|p| layout.sequences[p.sequence].groups[p.group].play_count == 0),
            players: props
                .iter()
                .map(|p| sprite_sequence::Player::new(&layout.sequences[p.sequence], p.group))
                .collect(),
        }
    }

    /// One frame of `step` (1.0 per 60 Hz frame).
    pub fn update(&mut self, layout: &Layout, step: f32) {
        match self.state {
            1 => {
                self.state = 2;
                self.time = 0.;
            }
            2 => {
                self.time += step;
                if self.end + 1. <= self.time && !self.looping {
                    self.state = 3;
                    self.time = self.end;
                }
            }
            _ => return,
        }
        let props = &layout.props[layout.scenes[self.scene].props.clone()];
        let mut busy = false;
        for (p, player) in props.iter().zip(&mut self.players) {
            if self.time < p.show_at as f32 {
                busy = true;
            } else {
                player.update(&layout.sequences[p.sequence], step);
                busy |= !player.finished();
            }
        }
        if !busy {
            self.state = 3;
        }
    }

    pub fn alive(&self) -> bool {
        matches!(self.state, 1 | 2) && (self.looping || self.time < self.end + 1.)
    }

    /// Props shown now, in draw order: (prop, its player, draw origin).
    pub fn visible<'a>(
        &'a self,
        layout: &'a Layout,
        offset: [i32; 2],
    ) -> impl Iterator<Item = (&'a Prop, &'a sprite_sequence::Player, [f32; 2])> {
        let shown = matches!(self.state, 2 | 3);
        layout.props[layout.scenes[self.scene].props.clone()]
            .iter()
            .zip(&self.players)
            .filter(move |(p, _)| shown && p.show_at as f32 <= self.time)
            .map(move |(p, player)| {
                let at = [p.position[0] + offset[0], p.position[1] + offset[1]];
                (p, player, at.map(|v| v as f32))
            })
    }
}

/// Tz::Layout (update 0x1402ec5a0): a scene player with a queued next
/// scene; when a scene ends it switches to the queued one (raising
/// `just_switched` for that frame) or dies.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutTask {
    pub player: ScenePlayer,
    /// Current scene (+0xe0); set at once by `set_scene`, played from the
    /// next update.
    pub scene: usize,
    pub next: Option<usize>,
    pub just_switched: bool,
    pub dead: bool,
    pending: Option<usize>,
}

impl LayoutTask {
    pub fn new(layout: &Layout, scene: usize) -> Self {
        Self {
            player: ScenePlayer::new(layout, scene),
            scene,
            next: None,
            just_switched: false,
            dead: false,
            pending: Some(scene),
        }
    }

    /// Switch scenes from the next update on.
    pub fn set_scene(&mut self, scene: usize, next: Option<usize>) {
        self.scene = scene;
        self.pending = Some(scene);
        self.next = next;
    }

    pub fn kill(&mut self) {
        self.dead = true;
    }

    pub fn update(&mut self, layout: &Layout, step: f32) {
        if self.dead {
            return;
        }
        if let Some(scene) = self.pending.take() {
            self.player = ScenePlayer::new(layout, scene);
            self.just_switched = false;
        }
        self.player.update(layout, step);
        self.just_switched = false;
        if self.player.alive() {
            return;
        }
        match self.next.take() {
            Some(scene) => {
                self.scene = scene;
                self.player = ScenePlayer::new(layout, scene);
                self.player.update(layout, step);
                self.just_switched = true;
            }
            None => self.dead = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(b: &mut [u8], at: usize, v: i32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A minimal SEQD: one sprite, part and sprite group; one animation
    /// over frames 0..=`end`; one group with `plays` plays (0 loops).
    fn seqd(end: i32, plays: i16) -> Vec<u8> {
        let mut b = vec![0u8; 260 + 0x24];
        b[..4].copy_from_slice(b"SEQD");
        // sprites @48, parts @92, sprite groups @112, animations @116,
        // groups @260.
        for (at, o) in [(8, 48), (16, 92), (24, 112), (32, 116), (40, 260)] {
            put(&mut b, at, 1);
            put(&mut b, at + 4, o);
        }
        b[112..116].copy_from_slice(&[0, 0, 1, 0]);
        put(&mut b, 116, sprite_sequence::LINEAR);
        put(&mut b, 116 + 12, end);
        b[260..264].copy_from_slice(&[0, 0, 1, 0]);
        b[264..266].copy_from_slice(&plays.to_le_bytes());
        b
    }

    /// Two props on SEQD 0 group 0 (shown at 0 and `late`), one scene.
    fn layd(late: i32, end: i32, plays: i16) -> Vec<u8> {
        let mut b = vec![0u8; 0x88];
        b[..4].copy_from_slice(b"LAYD");
        for (at, v) in [(8, 2), (12, 0x40), (16, 1), (20, 0x70), (24, 1), (28, 0x84)] {
            put(&mut b, at, v);
        }
        for (k, v) in [3, 0, 0, late, 10, -20].into_iter().enumerate() {
            put(&mut b, 0x58 + 4 * k, v);
        }
        b[0x70..0x74].copy_from_slice(&[0, 0, 2, 0]);
        put(&mut b, 0x84, 0x88);
        b.extend(seqd(end, plays));
        b
    }

    #[test]
    fn reads_props_scenes_and_sequences() {
        let mut b = layd(60, 10, 1);
        let l = read_layd(&b).unwrap();
        assert_eq!(l.scenes[0].props, 0..2);
        assert_eq!(
            l.props[1],
            Prop {
                texture: 3,
                sequence: 0,
                group: 0,
                show_at: 60,
                position: [10, -20]
            }
        );
        assert_eq!(l.sequences.len(), 1);
        b[0x58 + 8] = 5;
        assert!(read_layd(&b).is_err());
    }

    #[test]
    fn scene_ends_after_its_latest_prop() {
        let l = read_layd(&layd(5, 10, 1)).unwrap();
        // end = 5 + (10 + 1) - 1 = 15: alive through time 15.
        let mut s = ScenePlayer::new(&l, 0);
        let mut frames = 0;
        while s.alive() {
            s.update(&l, 1.);
            frames += 1;
            let shown = s.visible(&l, [0, 0]).count();
            assert_eq!(shown, if s.time < 5. { 1 } else { 2 }, "{frames}");
        }
        assert_eq!((frames, s.time), (17, 15.));
        assert_eq!(s.visible(&l, [1, 2]).nth(1).unwrap().2, [11., -18.]);
        let looping = read_layd(&layd(5, 10, 0)).unwrap();
        let mut s = ScenePlayer::new(&looping, 0);
        (0..100).for_each(|_| s.update(&looping, 1.));
        assert!(s.alive());
    }

    #[test]
    fn task_switches_to_its_queued_scene_once() {
        let l = read_layd(&layd(0, 3, 1)).unwrap();
        let mut t = LayoutTask::new(&l, 0);
        t.next = Some(0);
        let mut switched = vec![];
        for f in 0..12 {
            t.update(&l, 1.);
            if t.just_switched {
                switched.push(f);
            }
        }
        // Frames 0..=3 play the first pass (time 0..=3); on frame 4 it ends
        // and the queued scene starts; that pass ends on frame 8 and the
        // task dies.
        assert_eq!(switched, vec![4]);
        assert!(t.dead);
    }
}
