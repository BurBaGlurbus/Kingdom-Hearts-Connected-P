//! Playable TEXA texture animation of a model's `tim_` chunk: the id map,
//! the per-animation frame records and the replacement image strip that a
//! script's `SeqTextureAnim` (0x19) selects by number (exe kn::TEXAnmWork,
//! FUN_1401c0130 / FUN_1401c06b0 / FUN_1401c0750; private screen-spec 5).
//!
//! Chunk body: `u16 h[12]; i32 id_map, animations, pixels, default`.
//! h1 = texture environment, h3 = bits per pixel (8 here), h4/h5 = id range,
//! h6 = animation count, h7 = image count, h8/h9 = destination x/y in the
//! texture, h10/h11 = image size. Frame record (8 bytes): `i16 flags,
//! u16 dur_min, u16 dur_max, u16 image`; low nibble of flags 0 = show the
//! image for `dur_min` ticks, 1 = show it for a random time in
//! `dur_min..=dur_max` (the exe draws from the unseeded CRT `rand`, so a
//! blink's period is not reproducible; the midpoint stands in), 2 = jump
//! `flags >> 4` records, 3 = stop.
use crate::model_texture;
use std::io;

const MAX_RECORDS: usize = 4096;

/// One frame record of an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub control: u8,
    /// Signed record offset of a jump (control 2).
    pub jump: i32,
    pub min_ticks: u16,
    pub max_ticks: u16,
    pub image: u16,
}

/// One TEXA chunk: a patch rectangle of one texture and its animations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub environment: usize,
    pub min_id: u16,
    pub max_id: u16,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub id_map: Vec<u16>,
    pub animations: Vec<Vec<Record>>,
    /// 8-bit palette indices, `width * height` per image.
    pub images: Vec<Vec<u8>>,
    /// The model's default (idle) animation, when it names one.
    pub default_animation: Option<usize>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn u16_at(body: &[u8], offset: usize) -> io::Result<u16> {
    body.get(offset..offset + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| invalid("TEXA field outside its chunk"))
}

fn offset_at(body: &[u8], at: usize) -> io::Result<usize> {
    let raw = body
        .get(at..at + 4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| invalid("TEXA header short"))?;
    usize::try_from(raw).map_err(|_| invalid("negative TEXA pointer"))
}

/// Parses one TEXA chunk body; `None` for depths other than 8 bits.
pub fn read_body(body: &[u8]) -> io::Result<Option<Patch>> {
    let h = |i: usize| u16_at(body, i * 2);
    if body.len() < 40 {
        return Err(invalid("short TEXA header"));
    }
    if h(3)? != 8 {
        return Ok(None);
    }
    let (min_id, max_id) = (h(4)?, h(5)?);
    let animation_count = h(6)? as usize;
    let image_count = h(7)? as usize;
    let (width, height) = (h(10)? as u32, h(11)? as u32);
    if max_id < min_id || animation_count > 1024 || image_count > 1024 {
        return Err(invalid("TEXA counts outside bounds"));
    }
    let id_map_at = offset_at(body, 24)?;
    let animations_at = offset_at(body, 28)?;
    let pixels_at = offset_at(body, 32)?;
    let default = i32::from_le_bytes(body[36..40].try_into().unwrap());
    let id_map = (0..=(max_id - min_id) as usize)
        .map(|i| u16_at(body, id_map_at + i * 2))
        .collect::<io::Result<Vec<_>>>()?;
    let mut animations = Vec::with_capacity(animation_count);
    for a in 0..animation_count {
        let first = offset_at(body, animations_at + a * 4)?;
        let mut records = Vec::new();
        loop {
            let at = first + records.len() * 8;
            let raw = body
                .get(at..at + 8)
                .ok_or_else(|| invalid("TEXA frame record outside its chunk"))?;
            let flags = i16::from_le_bytes([raw[0], raw[1]]);
            let control = (flags & 15) as u8;
            let record = Record {
                control,
                jump: (flags >> 4) as i32,
                min_ticks: u16::from_le_bytes([raw[2], raw[3]]),
                max_ticks: u16::from_le_bytes([raw[4], raw[5]]),
                image: u16::from_le_bytes([raw[6], raw[7]]),
            };
            records.push(record);
            if control == 3 || (control == 2 && record.jump <= 0) {
                break;
            }
            if records.len() >= MAX_RECORDS {
                return Err(invalid("TEXA animation has no terminating record"));
            }
        }
        animations.push(records);
    }
    let size = (width * height) as usize;
    let mut images = Vec::with_capacity(image_count);
    for i in 0..image_count {
        let start = pixels_at + i * size;
        images.push(
            body.get(start..start + size)
                .ok_or_else(|| invalid("TEXA image outside its chunk"))?
                .to_vec(),
        );
    }
    let default_animation = usize::try_from(default)
        .ok()
        .filter(|&d| d < animation_count);
    Ok(Some(Patch {
        environment: h(1)? as usize,
        min_id,
        max_id,
        x: h(8)? as u32,
        y: h(9)? as u32,
        width,
        height,
        id_map,
        animations,
        images,
        default_animation,
    }))
}

/// Every 8-bit TEXA chunk of a `tim_` payload.
pub fn read_patches(bytes: &[u8]) -> io::Result<Vec<Patch>> {
    let layout = model_texture::read_layout(bytes)?;
    let mut patches = Vec::new();
    for chunk in layout.footer.iter().filter(|c| &c.tag == b"TEXA") {
        let start = chunk.offset + 8;
        let body = bytes
            .get(start..start + chunk.length)
            .ok_or_else(|| invalid("TEXA chunk exceeds the texture"))?;
        patches.extend(read_body(body)?);
    }
    Ok(patches)
}

impl Patch {
    /// The animation a script number selects (`id_map` is 1-based).
    pub fn animation_for(&self, number: u16) -> Option<usize> {
        if number < self.min_id || number > self.max_id {
            return None;
        }
        match self.id_map[(number - self.min_id) as usize] {
            0 => None,
            n => Some(n as usize - 1),
        }
        .filter(|&a| a < self.animations.len())
    }

    /// The image index an animation shows `ticks` after it started
    /// (random durations take their midpoint).
    pub fn image_at(&self, animation: usize, ticks: f32) -> Option<u16> {
        let records = self.animations.get(animation)?;
        let (mut index, mut t) = (0usize, 0f32);
        let mut shown = None;
        let mut idle_steps = 0usize;
        for _ in 0..1_000_000 {
            let r = records.get(index)?;
            match r.control {
                0 | 1 => {
                    let d = if r.control == 1 {
                        (r.min_ticks as f32 + r.max_ticks as f32) / 2.
                    } else {
                        r.min_ticks as f32
                    };
                    shown = Some(r.image);
                    if d > 0. {
                        idle_steps = 0;
                        if ticks < t + d {
                            return shown;
                        }
                        t += d;
                    } else {
                        idle_steps += 1;
                        if idle_steps > records.len() * 2 {
                            return shown;
                        }
                    }
                    index += 1;
                }
                2 => {
                    let next = index as i64 + r.jump as i64;
                    if next < 0 || next as usize >= records.len() {
                        return shown;
                    }
                    idle_steps += 1;
                    if idle_steps > records.len() * 2 {
                        return shown;
                    }
                    index = next as usize;
                }
                _ => return shown,
            }
        }
        shown
    }

    /// Writes one image into a decoded RGBA texture of `texture_width`
    /// through the model's palette (`clut_rgba` = 256 RGBA entries).
    pub fn paint(&self, rgba: &mut [u8], texture_width: u32, image: u16, clut_rgba: &[u8]) {
        let Some(pixels) = self.images.get(image as usize) else {
            return;
        };
        for row in 0..self.height {
            for col in 0..self.width {
                let index = pixels[(row * self.width + col) as usize] as usize;
                let to = (((self.y + row) * texture_width + self.x + col) * 4) as usize;
                if let (Some(dst), Some(src)) = (
                    rgba.get_mut(to..to + 4),
                    clut_rgba.get(index * 4..index * 4 + 4),
                ) {
                    dst.copy_from_slice(src);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        // id range 4..=6, 2 animations, 2 images of 2x1.
        let mut b = vec![0u8; 0x100];
        let put16 =
            |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        for (i, v) in [2u16, 1, 0, 8, 4, 6, 2, 2, 3, 1, 2, 1]
            .into_iter()
            .enumerate()
        {
            put16(&mut b, i * 2, v);
        }
        b[24..28].copy_from_slice(&0x40u32.to_le_bytes()); // id map
        b[28..32].copy_from_slice(&0x50u32.to_le_bytes()); // animations
        b[32..36].copy_from_slice(&0xe0u32.to_le_bytes()); // pixels
        b[36..40].copy_from_slice(&1i32.to_le_bytes()); // default
        for (i, v) in [0u16, 2, 1].into_iter().enumerate() {
            put16(&mut b, 0x40 + i * 2, v);
        }
        b[0x50..0x54].copy_from_slice(&0x60u32.to_le_bytes());
        b[0x54..0x58].copy_from_slice(&0x90u32.to_le_bytes());
        // anim 0: image 1 for 4 ticks, image 0 for 6 ticks, loop -2.
        let rec = |b: &mut Vec<u8>, at: usize, f: i16, lo: u16, hi: u16, img: u16| {
            b[at..at + 2].copy_from_slice(&f.to_le_bytes());
            b[at + 2..at + 4].copy_from_slice(&lo.to_le_bytes());
            b[at + 4..at + 6].copy_from_slice(&hi.to_le_bytes());
            b[at + 6..at + 8].copy_from_slice(&img.to_le_bytes());
        };
        rec(&mut b, 0x60, 0, 4, 4, 1);
        rec(&mut b, 0x68, 0, 6, 6, 0);
        rec(&mut b, 0x70, -2 << 4 | 2, 0, 0, 0);
        // anim 1: hold image 1 then stop.
        rec(&mut b, 0x90, 0, 2, 2, 1);
        rec(&mut b, 0x98, 3, 0, 0, 0);
        b[0xe0..0xe4].copy_from_slice(&[10, 11, 20, 21]);
        b
    }

    #[test]
    fn parses_ids_records_and_images() {
        let p = read_body(&fixture()).unwrap().unwrap();
        assert_eq!(
            (p.environment, p.x, p.y, p.width, p.height),
            (1, 3, 1, 2, 1)
        );
        assert_eq!(p.animation_for(3), None);
        assert_eq!(p.animation_for(4), None);
        assert_eq!(p.animation_for(5), Some(1));
        assert_eq!(p.animation_for(6), Some(0));
        assert_eq!(p.animations[0].len(), 3);
        assert_eq!(p.animations[0][2].jump, -2);
        assert_eq!(p.images, vec![vec![10, 11], vec![20, 21]]);
        assert_eq!(p.default_animation, Some(1));
    }

    #[test]
    fn loops_and_stops_by_elapsed_ticks() {
        let p = read_body(&fixture()).unwrap().unwrap();
        assert_eq!(p.image_at(0, 0.), Some(1));
        assert_eq!(p.image_at(0, 3.9), Some(1));
        assert_eq!(p.image_at(0, 4.), Some(0));
        assert_eq!(p.image_at(0, 10.), Some(1));
        assert_eq!(p.image_at(0, 14.5), Some(0));
        assert_eq!(p.image_at(1, 500.), Some(1));
    }

    #[test]
    fn random_durations_use_their_midpoint() {
        let mut b = fixture();
        // anim 1, first record: random 10..=20 ticks of image 1.
        b[0x90..0x92].copy_from_slice(&1i16.to_le_bytes());
        b[0x92..0x94].copy_from_slice(&10u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&20u16.to_le_bytes());
        let p = read_body(&b).unwrap().unwrap();
        assert_eq!(p.animations[1][0].control, 1);
        assert_eq!(p.image_at(1, 14.9), Some(1));
        assert_eq!(p.image_at(1, 15.), Some(1)); // stop holds the last image
    }

    #[test]
    fn paints_the_patch_through_the_palette() {
        let p = read_body(&fixture()).unwrap().unwrap();
        let clut: Vec<u8> = (0..=255u8).flat_map(|i| [i, 0, 0, 255]).collect();
        let mut rgba = vec![0u8; 8 * 4 * 4];
        p.paint(&mut rgba, 8, 1, &clut);
        let at = ((8 + 3) * 4) as usize;
        assert_eq!(&rgba[at..at + 8], &[20, 0, 0, 255, 21, 0, 0, 255]);
        assert!(rgba[..at].iter().all(|&v| v == 0));
    }

    #[test]
    fn rejects_unterminated_and_truncated_chunks() {
        let mut b = fixture();
        b[0x70..0x72].copy_from_slice(&0i16.to_le_bytes());
        b[0x98..0x9a].copy_from_slice(&0i16.to_le_bytes());
        assert!(read_body(&b).is_err());
        assert!(read_body(&fixture()[..30]).is_err());
    }

    #[test]
    fn other_depths_are_not_patched() {
        let mut b = fixture();
        b[6..8].copy_from_slice(&4u16.to_le_bytes());
        assert_eq!(read_body(&b).unwrap(), None);
    }
}
