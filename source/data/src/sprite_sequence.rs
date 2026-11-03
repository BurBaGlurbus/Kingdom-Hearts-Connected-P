//! SEQD 2D sequences (HUD and menu sprites). Table layout follows the public
//! OpenKH `Sequence.cs` (7a3b945c538d32c6a285128c98aefba093f52ceb). Playback
//! is translated from the PC executable's sequence player: time stepping and
//! looping (0x140192130, 0x140192050, 0x140193280), the active-animation
//! test, Bezier easing and child-context publication (0x140193880,
//! 0x140192250).
use std::io;

pub const LINEAR: i32 = 0x1;
pub const IS_ACTIVE: i32 = 0x2;
pub const BOUNCE_COSINE: i32 = 0x8;
pub const BOUNCE_DISABLE: i32 = 0x10;
pub const ROTATION_DISABLE: i32 = 0x20;
pub const SCALING_DISABLE: i32 = 0x40;
pub const COLOR_START_ONLY: i32 = 0x80;
pub const COLOR_MASK: i32 = 0x400;
pub const TRANSLATION_START_ONLY: i32 = 0x1000;
pub const PIVOT_DISABLE: i32 = 0x4000;
pub const LAST_CUT: i32 = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    /// Texture rectangle in original (pre-HD) texel units.
    pub uv: [i32; 4],
    pub scroll: [f32; 2],
    /// Corner colors (left/top/right/bottom), RGBA bytes where 0x80 is 1.0.
    pub colors: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpritePart {
    pub rect: [i32; 4],
    pub sprite: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub flags: i32,
    pub sprite_group: usize,
    pub frames: [i32; 2],
    pub translate: [i32; 4],
    pub pivot: [i32; 4],
    /// X, Y, Z (start, end) in radians.
    pub rotation: [f32; 6],
    pub scale: [f32; 2],
    pub scale_x: [f32; 2],
    pub scale_y: [f32; 2],
    /// Easing control values (X start, Y start, X end, Y end).
    pub curve: [f32; 4],
    pub bounce: [f32; 4],
    pub bounce_count: [i16; 2],
    pub blend: i32,
    pub color: [u32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationGroup {
    pub animations: std::ops::Range<usize>,
    /// Play count; 0 loops forever.
    pub play_count: i16,
    pub loop_frames: [i32; 2],
    /// Group +0x10 / +0x14 (OpenKH LightPositionX / TextPositionY): the
    /// message window uses them as the box's interior padding.
    pub light_position_x: i32,
    pub text_position_y: i32,
    /// +0x18 TextScale (text size; 0 = the caller's default), +0x1c
    /// UiPadding, +0x20 TextPositionX (the camp lists use it as a pitch).
    pub text_scale: i32,
    pub ui_padding: i32,
    pub text_position_x: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    pub sprites: Vec<Sprite>,
    pub parts: Vec<SpritePart>,
    pub sprite_groups: Vec<std::ops::Range<usize>>,
    pub animations: Vec<Animation>,
    pub groups: Vec<AnimationGroup>,
}

/// One textured quad: corners (top-left, top-right, bottom-left,
/// bottom-right), texel rectangle and RGBA multipliers per corner (1.0 =
/// unchanged).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub corners: [[f32; 2]; 4],
    pub uv: [i32; 4],
    pub colors: [[f32; 4]; 4],
    pub blend: i32,
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn i16_at(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

pub fn read(bytes: &[u8]) -> io::Result<Sequence> {
    let invalid = |m| io::Error::new(io::ErrorKind::InvalidData, m);
    if bytes.len() < 48 || !bytes.starts_with(b"SEQD") || bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("SEQD signature/bounds"));
    }
    let table = |at: usize, stride: usize| -> io::Result<(usize, usize)> {
        let (count, offset) = (i32_at(bytes, at), i32_at(bytes, at + 4));
        let (count, offset) = (
            usize::try_from(count).map_err(|_| invalid("negative SEQD count"))?,
            usize::try_from(offset).map_err(|_| invalid("negative SEQD offset"))?,
        );
        if offset + count * stride > bytes.len() {
            return Err(invalid("SEQD table exceeds file"));
        }
        Ok((count, offset))
    };
    let (sc, so) = table(8, 44)?;
    let (pc, po) = table(16, 20)?;
    let (gc, go) = table(24, 4)?;
    let (ac, ao) = table(32, 0x90)?;
    let (agc, ago) = table(40, 0x24)?;
    let sprites = (0..sc)
        .map(|i| {
            let a = so + i * 44;
            Sprite {
                uv: std::array::from_fn(|k| i32_at(bytes, a + 4 + k * 4)),
                scroll: [f32_at(bytes, a + 20), f32_at(bytes, a + 24)],
                colors: std::array::from_fn(|k| i32_at(bytes, a + 28 + k * 4) as u32),
            }
        })
        .collect::<Vec<_>>();
    let parts = (0..pc)
        .map(|i| {
            let a = po + i * 20;
            usize::try_from(i32_at(bytes, a + 16))
                .ok()
                .filter(|s| *s < sc)
                .map(|sprite| SpritePart {
                    rect: std::array::from_fn(|k| i32_at(bytes, a + k * 4)),
                    sprite,
                })
                .ok_or_else(|| invalid("SEQD part sprite outside table"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let range = |start: i64, count: i64, len: usize| -> io::Result<std::ops::Range<usize>> {
        if start < 0 || count < 0 || (start + count) as usize > len {
            return Err(invalid("SEQD range outside table"));
        }
        Ok(start as usize..(start + count) as usize)
    };
    let sprite_groups = (0..gc)
        .map(|i| {
            let a = go + i * 4;
            range(i16_at(bytes, a) as i64, i16_at(bytes, a + 2) as i64, pc)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let animations = (0..ac)
        .map(|i| {
            let a = ao + i * 0x90;
            let int = |k: usize| i32_at(bytes, a + k * 4);
            let float = |k: usize| f32_at(bytes, a + 48 + k * 4);
            let sprite_group = usize::try_from(int(1))
                .ok()
                .filter(|g| *g < gc)
                .ok_or_else(|| invalid("SEQD animation sprite group outside table"))?;
            Ok(Animation {
                flags: int(0),
                sprite_group,
                frames: [int(2), int(3)],
                translate: [int(4), int(5), int(6), int(7)],
                pivot: [int(8), int(9), int(10), int(11)],
                rotation: std::array::from_fn(float),
                scale: [float(6), float(7)],
                scale_x: [float(8), float(9)],
                scale_y: [float(10), float(11)],
                curve: [float(12), float(13), float(14), float(15)],
                bounce: [float(16), float(17), float(18), float(19)],
                bounce_count: [i16_at(bytes, a + 128), i16_at(bytes, a + 130)],
                blend: i32_at(bytes, a + 132),
                color: [i32_at(bytes, a + 136) as u32, i32_at(bytes, a + 140) as u32],
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let groups = (0..agc)
        .map(|i| {
            let a = ago + i * 0x24;
            Ok(AnimationGroup {
                animations: range(i16_at(bytes, a) as i64, i16_at(bytes, a + 2) as i64, ac)?,
                play_count: i16_at(bytes, a + 4),
                loop_frames: [i32_at(bytes, a + 8), i32_at(bytes, a + 12)],
                light_position_x: i32_at(bytes, a + 16),
                text_position_y: i32_at(bytes, a + 20),
                text_scale: i32_at(bytes, a + 24),
                ui_padding: i32_at(bytes, a + 28),
                text_position_x: i32_at(bytes, a + 32),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Sequence {
        sprites,
        parts,
        sprite_groups,
        animations,
        groups,
    })
}

impl Sequence {
    /// FUN_140193430: fits the nine parts of the sprite group drawn by
    /// `group`'s first animation around a `w` x `h` interior (each padded
    /// by the group's +0x10 / +0x14 and rounded down to even), centred on the
    /// origin: corners keep their size, edges and centre stretch.
    pub fn fit_nine_slice(&mut self, group: usize, w: i32, h: i32) {
        let g = &self.groups[group];
        let (w, h) = ((g.light_position_x + w) & !1, (g.text_position_y + h) & !1);
        let Some(a) = self.animations.get(g.animations.start) else {
            return;
        };
        let parts = self.sprite_groups[a.sprite_group].clone();
        if parts.len() < 9 {
            return;
        }
        let p = &mut self.parts[parts];
        let width = |p: &SpritePart| p.rect[2] - p.rect[0];
        let height = |p: &SpritePart| p.rect[3] - p.rect[1];
        let (left, top, right, bottom) = (-(w / 2), -(h / 2), w / 2, h / 2);
        // (x, y, width, height) per part; None keeps the part's own size.
        let layout: [(i32, i32, Option<i32>, Option<i32>); 9] = [
            (left - width(&p[0]), top - height(&p[0]), None, None),
            (left, top - height(&p[1]), Some(w), None),
            (right, top - height(&p[2]), None, None),
            (left - width(&p[3]), top, None, Some(h)),
            (left, top, Some(w), Some(h)),
            (right, top, None, Some(h)),
            (left - width(&p[6]), bottom, None, None),
            (left, bottom, Some(w), None),
            (right, bottom, None, None),
        ];
        for (part, (x, y, pw, ph)) in p.iter_mut().zip(layout) {
            let pw = pw.unwrap_or(width(part));
            let ph = ph.unwrap_or(height(part));
            part.rect = [x, y, x + pw, y + ph];
        }
    }
}

/// Sequence colors: 0x80 per channel is 1.0.
pub fn color(c: u32) -> [f32; 4] {
    std::array::from_fn(|k| ((c >> (8 * k)) & 0xff) as f32 / 128.)
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a * (1. - t) + b * t
}

/// Playback state of one group (the player embedded in each sprite).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub group: usize,
    pub time: f32,
    /// 1 = just set, 2 = playing, 3 = finished.
    pub state: u8,
    pub plays_left: i16,
}

impl Player {
    pub fn new(sequence: &Sequence, group: usize) -> Self {
        Self {
            group,
            time: 0.,
            state: 1,
            plays_left: sequence.groups[group].play_count,
        }
    }

    /// The group's last frame: the latest animation end (one less for
    /// last-cut animations), or the group's loop end when set.
    fn end(sequence: &Sequence, group: usize) -> i32 {
        let g = &sequence.groups[group];
        if g.loop_frames[1] != 0 {
            return g.loop_frames[1];
        }
        sequence.animations[g.animations.clone()]
            .iter()
            .map(|a| a.frames[1] - (a.flags & LAST_CUT != 0) as i32)
            .max()
            .unwrap_or(0)
    }

    /// One frame. The first update after a set only starts playback.
    pub fn update(&mut self, sequence: &Sequence, step: f32) {
        match self.state {
            1 => self.state = 2,
            2 => {
                self.time += step;
                let limit = (Self::end(sequence, self.group) + 1) as f32;
                if self.time >= limit {
                    self.time = sequence.groups[self.group].loop_frames[0] as f32;
                    if self.plays_left != 0 {
                        self.plays_left -= 1;
                        if self.plays_left == 0 {
                            self.state = 3;
                            self.time = limit - 1.;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn finished(&self) -> bool {
        self.state == 3
    }

    /// Animations active now with their eased progress.
    fn active<'a>(&self, sequence: &'a Sequence) -> impl Iterator<Item = (&'a Animation, f32)> {
        let t = self.time;
        sequence.animations[sequence.groups[self.group].animations.clone()]
            .iter()
            .filter_map(move |a| {
                let [start, end] = a.frames.map(|f| f as f32);
                let inside = if a.flags & LAST_CUT != 0 {
                    t >= start && t < end
                } else {
                    t >= start && t <= end
                };
                inside.then(|| {
                    let p = if end > start {
                        (t - start) / (end - start)
                    } else {
                        1.
                    };
                    let eased = if a.flags & LINEAR != 0 {
                        p
                    } else {
                        let q = 1. - p;
                        3. * q * q * p * a.curve[1] + 3. * q * p * p * a.curve[3] + p * p * p
                    };
                    (a, eased)
                })
            })
    }
}

/// External multipliers a sprite object applies on top of its animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifiers {
    pub scale: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Extra Z rotation (same sense as the animation's).
    pub rotation: f32,
    pub color: [f32; 4],
}

impl Default for Modifiers {
    fn default() -> Self {
        Self {
            scale: 1.,
            scale_x: 1.,
            scale_y: 1.,
            rotation: 0.,
            color: [1.; 4],
        }
    }
}

struct Frame {
    translate: [f32; 2],
    pivot: [f32; 2],
    scale: [f32; 2],
    rotation: [f32; 3],
    color: [f32; 4],
}

fn frame(a: &Animation, t: f32, m: &Modifiers) -> Frame {
    let tr = a.translate.map(|v| v as f32);
    let mut translate = if a.flags & TRANSLATION_START_ONLY != 0 {
        [tr[0], tr[2]]
    } else {
        [lerp(t, tr[0], tr[1]), lerp(t, tr[2], tr[3])]
    };
    if a.flags & BOUNCE_DISABLE == 0 {
        let wave = |count: i16| {
            let x = count as f32 * t * std::f32::consts::PI;
            if a.flags & BOUNCE_COSINE != 0 {
                x.cos()
            } else {
                x.sin()
            }
        };
        translate[0] += wave(a.bounce_count[0]) * lerp(t, a.bounce[0], a.bounce[1]);
        translate[1] += wave(a.bounce_count[1]) * lerp(t, a.bounce[2], a.bounce[3]);
    }
    let scale = if a.flags & SCALING_DISABLE == 0 {
        let s = lerp(t, a.scale[0], a.scale[1]) * m.scale;
        [
            s * lerp(t, a.scale_x[0], a.scale_x[1]) * m.scale_x,
            s * lerp(t, a.scale_y[0], a.scale_y[1]) * m.scale_y,
        ]
    } else {
        [m.scale * m.scale_x, m.scale * m.scale_y]
    };
    let pivot = if a.flags & PIVOT_DISABLE == 0 {
        let p = a.pivot.map(|v| v as f32);
        [lerp(t, p[0], p[1]), lerp(t, p[2], p[3])]
    } else {
        [0., 0.]
    };
    let rotation = if a.flags & ROTATION_DISABLE == 0 {
        let r = a.rotation;
        [
            lerp(t, r[0], r[1]),
            lerp(t, r[2], r[3]),
            lerp(t, r[4], r[5]) + m.rotation,
        ]
    } else {
        [0., 0., m.rotation]
    };
    let tint = if a.flags & COLOR_MASK != 0 {
        [1.; 4]
    } else if a.flags & COLOR_START_ONLY != 0 {
        color(a.color[0])
    } else {
        let (c0, c1) = (color(a.color[0]), color(a.color[1]));
        std::array::from_fn(|k| lerp(t, c0[k], c1[k]))
    };
    Frame {
        translate,
        pivot,
        scale,
        rotation,
        color: std::array::from_fn(|k| tint[k] * m.color[k]),
    }
}

/// Screen-space transform of a local point: scale, then X/Y rotations as
/// orthographic foreshortening, then Z rotation (x cos + y sin, -x sin +
/// y cos; Y points down, so a negative angle turns clockwise).
fn transform(f: &Frame, x: f32, y: f32) -> [f32; 2] {
    let (x, y) = (x * f.scale[0], y * f.scale[1]);
    let (x, y) = (x * f.rotation[1].cos(), y * f.rotation[0].cos());
    let (s, c) = f.rotation[2].sin_cos();
    [x * c + y * s, -x * s + y * c]
}

impl Sequence {
    /// Child context of a player: the first active IsActive animation's
    /// translation (with bounce) plus its scaled, rotated pivot. Relative to
    /// the player's own draw origin.
    pub fn child_context(&self, player: &Player, m: &Modifiers) -> Option<[f32; 2]> {
        player
            .active(self)
            .find(|(a, _)| a.flags & IS_ACTIVE != 0)
            .map(|(a, t)| {
                let f = frame(a, t, m);
                let p = if a.flags & ROTATION_DISABLE != 0 {
                    [f.pivot[0] * f.scale[0], f.pivot[1] * f.scale[1]]
                } else {
                    transform(&f, f.pivot[0], f.pivot[1])
                };
                [f.translate[0] + p[0], f.translate[1] + p[1]]
            })
    }

    /// Colour of the first active IsActive animation (the one that gives
    /// the child context, FUN_140192be0), with the modifiers applied.
    pub fn context_color(&self, player: &Player, m: &Modifiers) -> Option<[f32; 4]> {
        player
            .active(self)
            .find(|(a, _)| a.flags & IS_ACTIVE != 0)
            .map(|(a, t)| frame(a, t, m).color)
    }

    /// Colour of the first active animation (the player's current colour).
    pub fn current_color(&self, player: &Player) -> Option<[f32; 4]> {
        player
            .active(self)
            .next()
            .map(|(a, t)| frame(a, t, &Modifiers::default()).color)
    }

    /// Quads of a player's current frame drawn at `origin`.
    pub fn quads(&self, player: &Player, origin: [f32; 2], m: &Modifiers) -> Vec<Quad> {
        let mut out = Vec::new();
        for (a, t) in player.active(self) {
            let f = frame(a, t, m);
            for part in &self.parts[self.sprite_groups[a.sprite_group].clone()] {
                let sprite = &self.sprites[part.sprite];
                let [l, top, r, b] = part.rect.map(|v| v as f32);
                let corners = [[l, top], [r, top], [l, b], [r, b]].map(|[x, y]| {
                    let p = transform(&f, x + f.pivot[0], y + f.pivot[1]);
                    [
                        origin[0] + f.translate[0] + p[0],
                        origin[1] + f.translate[1] + p[1],
                    ]
                });
                let corner = sprite.colors.map(color);
                out.push(Quad {
                    corners,
                    uv: sprite.uv,
                    colors: corner.map(|c| std::array::from_fn(|k| c[k] * f.color[k])),
                    blend: a.blend,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> {
        let mut b = vec![0u8; 48];
        b[..4].copy_from_slice(b"SEQD");
        let put =
            |b: &mut Vec<u8>, at: usize, v: i32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let so = b.len();
        b.extend(std::iter::repeat_n(0, 44));
        for (k, v) in [0, 10, 20, 30, 40].iter().enumerate() {
            put(&mut b, so + k * 4, *v);
        }
        for k in 0..4 {
            put(&mut b, so + 28 + k * 4, 0x8080_8080u32 as i32);
        }
        let po = b.len();
        b.extend(std::iter::repeat_n(0, 20));
        for (k, v) in [0, 0, 8, 4, 0].iter().enumerate() {
            put(&mut b, po + k * 4, *v);
        }
        let go = b.len();
        b.extend([0, 0, 1, 0]);
        let ao = b.len();
        b.extend(std::iter::repeat_n(0, 0x90));
        put(
            &mut b,
            ao,
            LINEAR | IS_ACTIVE | BOUNCE_DISABLE | ROTATION_DISABLE,
        );
        put(&mut b, ao + 12, 10);
        put(&mut b, ao + 20, 100);
        for k in 6..12 {
            b[ao + 48 + k * 4..ao + 52 + k * 4].copy_from_slice(&1f32.to_le_bytes());
        }
        put(&mut b, ao + 136, 0x8080_8080u32 as i32);
        put(&mut b, ao + 140, 0x0080_8080);
        let ago = b.len();
        b.extend(std::iter::repeat_n(0, 0x24));
        b[ago + 2] = 1;
        for (at, (count, offset)) in [(1, so), (1, po), (1, go), (1, ao), (1, ago)]
            .into_iter()
            .enumerate()
        {
            put(&mut b, 8 + at * 8, count);
            put(&mut b, 12 + at * 8, offset as i32);
        }
        b
    }

    #[test]
    fn plays_translates_and_publishes_child_context() {
        let seq = read(&synthetic()).unwrap();
        let mut player = Player::new(&seq, 0);
        // The first update only starts playback.
        player.update(&seq, 1.);
        assert_eq!(player.time, 0.);
        for _ in 0..5 {
            player.update(&seq, 1.);
        }
        let m = Modifiers::default();
        let q = seq.quads(&player, [1., 2.], &m)[0];
        assert_eq!(q.uv, [10, 20, 30, 40]);
        // Halfway: translate x 0 -> 100 gives 50.
        assert_eq!(q.corners[0], [51., 2.]);
        assert_eq!(q.corners[3], [59., 6.]);
        assert!((q.colors[0][3] - 0.5).abs() < 1e-6);
        assert_eq!(seq.child_context(&player, &m), Some([50., 0.]));
        // Looping group: after frame 10 the time returns to the loop start.
        for _ in 0..6 {
            player.update(&seq, 1.);
        }
        assert_eq!(player.time, 0.);
        assert!(!player.finished());
    }

    #[test]
    fn one_shot_groups_finish_on_their_last_frame() {
        let mut bytes = synthetic();
        let ago = bytes.len() - 0x24;
        bytes[ago + 4] = 1;
        let seq = read(&bytes).unwrap();
        let mut player = Player::new(&seq, 0);
        for _ in 0..12 {
            player.update(&seq, 1.);
        }
        assert!(player.finished());
        assert_eq!(player.time, 10.);
    }

    #[test]
    fn rejects_bad_signature_and_tables() {
        assert!(read(&[0u8; 64]).is_err());
        let mut b = synthetic();
        b[8..12].copy_from_slice(&1000i32.to_le_bytes());
        assert!(read(&b).is_err());
    }
}
