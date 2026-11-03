//! The look and sounds of KINGDOM HEARTS HD 1.5+2.5 ReMIX's own settings
//! menu, read from the player's install when the launcher starts:
//! `Image/SettingMenu.hed/.pkg`, its English layout (the gold-patterned
//! background and the atlas of bars, banner and pointer) and its menu sound
//! bank. Nothing is kept; without them the launcher draws plain shapes.
//!
//! [installed-build observation, KHC-059] The package has 118 entries; the
//! English group's layout is entry 74 (an L2D whose first TIM2 is the
//! 2048x2048 background, its 1920x1080 picture at (64, 484), and whose second
//! is the 2048x1388 atlas) and its sound bank entry 78. Which sound is which
//! is inferred from their lengths, not from the game's code.
use bevy::prelude::*;
use engine::launcher::paint::{self, Paint, quad, rgba};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use visual::overlay::Ui2dFrame;

const ENTRIES: usize = 118;
const LAYOUT: usize = 74;
const SOUND_BANK: usize = 78;
const LAYOUT_LIMIT: u32 = 80 << 20;
const BACKGROUND: [u32; 4] = [64, 484, 1920, 1080];
const ATLAS_SIZE: [u32; 2] = [2048, 1388];

/// Atlas pieces [x0, y0, x1, y1].
const BAR: [i32; 4] = [19, 121, 430, 164];
const BAR_ON: [i32; 4] = [14, 176, 419, 227];
const BANNER_LEFT: [i32; 4] = [11, 638, 107, 686];
const BANNER_FILL: [i32; 4] = [530, 638, 700, 686];
const BANNER_END: [i32; 4] = [700, 638, 784, 686];
const POINTER: [i32; 4] = [1535, 650, 1595, 691];

/// Menu sounds by their number in the bank [inferred from their lengths].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Cursor = 7,
    Decide = 2,
    Back = 3,
    Refused = 4,
}

/// What the loader thread read.
struct Read {
    background: Vec<u8>,
    atlas: Vec<u8>,
    sounds: Vec<Option<audio::Clip>>,
}

/// Where the loader thread leaves what it read.
type Slot = Arc<Mutex<Option<Result<Read, String>>>>;

#[derive(Default)]
pub struct CollectionMenu {
    job: Option<Slot>,
    /// The sheets once uploaded: background and atlas.
    sheets: Option<(usize, usize)>,
    pending: Option<Read>,
    sounds: Vec<Option<audio::Clip>>,
    output: Option<audio::Output>,
    /// Why the art is not there, once known.
    pub missing: Option<String>,
}

impl CollectionMenu {
    /// Starts reading from the game folder `root` (once).
    pub fn load(&mut self, root: &Path) {
        if self.job.is_some() {
            return;
        }
        let slot = Arc::new(Mutex::new(None));
        self.job = Some(slot.clone());
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            let got = read(&root);
            if let Ok(mut s) = slot.lock() {
                *s = Some(got);
            }
        });
    }

    /// Takes a finished read; true while there is art to draw with.
    pub fn ready(&mut self, images: &mut Assets<Image>, frame: &mut Ui2dFrame) -> bool {
        if let Some(slot) = &self.job
            && let Some(got) = slot.lock().ok().and_then(|mut s| s.take())
        {
            match got {
                Ok(mut read) => {
                    self.sounds = std::mem::take(&mut read.sounds);
                    self.pending = Some(read);
                    if self.output.is_none() {
                        self.output = audio::Output::open()
                            .inspect_err(|e| eprintln!("Menu sounds unavailable: {e}"))
                            .ok();
                    }
                }
                Err(why) => {
                    eprintln!("The 1.5+2.5 menu's look is not available: {why}");
                    self.missing = Some(why);
                }
            }
        }
        if let Some(read) = self.pending.take() {
            let size = [BACKGROUND[2], BACKGROUND[3]];
            let background = frame.add_sheet(images, read.background, size, 1.);
            let atlas = frame.add_sheet(images, read.atlas, ATLAS_SIZE, 1.);
            self.sheets = Some((background, atlas));
        }
        self.sheets.is_some()
    }

    pub fn play(&self, cue: Cue) {
        let (Some(out), Some(Some(clip))) = (&self.output, self.sounds.get(cue as usize)) else {
            return;
        };
        if let Ok(mut m) = out.mixer.lock() {
            m.play(clip.clone(), cue as u32, 0.8, 1.);
        }
    }

    /// The whole background, or a plain dark one.
    pub fn background(&self, p: &Paint, f: &mut Ui2dFrame) {
        match self.sheets {
            Some((sheet, _)) => {
                let uv = [0, 0, BACKGROUND[2] as i32, BACKGROUND[3] as i32];
                f.draws.push((
                    sheet,
                    vec![quad(
                        [0., 0., paint::W, paint::H],
                        uv,
                        [rgba(0xffffff, 1.); 4],
                        0,
                    )],
                ));
            }
            None => {
                let (top, bottom) = (rgba(0x1a1406, 1.), rgba(0x050403, 1.));
                p.rect(
                    f,
                    [0., 0., paint::W, paint::H],
                    [top, bottom, bottom, bottom],
                    false,
                );
            }
        }
    }

    /// The gold title band from the screen's left edge, `width` long.
    pub fn banner(&self, p: &Paint, f: &mut Ui2dFrame, [x, y, w, h]: [f32; 4]) {
        let Some((_, atlas)) = self.sheets else {
            p.plate(
                f,
                [x - 40., y, w + 40., h],
                h / 2.,
                0.,
                [rgba(0xb09858, 1.); 4],
                false,
            );
            return;
        };
        let scale = h / (BANNER_LEFT[3] - BANNER_LEFT[1]) as f32;
        let piece_w = |r: [i32; 4]| (r[2] - r[0]) as f32 * scale;
        let left = piece_w(BANNER_LEFT);
        let end = piece_w(BANNER_END);
        let white = [rgba(0xffffff, 1.); 4];
        f.draws.push((
            atlas,
            vec![
                quad([x, y, left, h], BANNER_LEFT, white, 0),
                quad(
                    [x + left, y, (w - left - end).max(0.), h],
                    BANNER_FILL,
                    white,
                    0,
                ),
                quad([x + w - end, y, end, h], BANNER_END, white, 0),
            ],
        ));
    }

    /// A row's bar: the gold one under the cursor, the grey one otherwise;
    /// stretched in the middle so the ends keep their shape.
    pub fn bar(&self, p: &Paint, f: &mut Ui2dFrame, r: [f32; 4], on: bool, alpha: f32) {
        let Some((_, atlas)) = self.sheets else {
            let (edge, fill) = if on {
                (0xe9c45f, 0x6a5a2a)
            } else {
                (0x55524a, 0x2a2926)
            };
            p.plate(f, r, 10., 0., [rgba(edge, alpha); 4], false);
            let [x, y, w, h] = r;
            p.plate(
                f,
                [x + 2., y + 2., w - 4., h - 4.],
                8.,
                0.,
                [rgba(fill, alpha); 4],
                false,
            );
            return;
        };
        let piece = if on { BAR_ON } else { BAR };
        let [x, y, w, h] = r;
        let scale = h / (piece[3] - piece[1]) as f32;
        let cap = 40;
        let cap_w = cap as f32 * scale;
        let left = [piece[0], piece[1], piece[0] + cap, piece[3]];
        let mid = [piece[0] + cap, piece[1], piece[2] - cap, piece[3]];
        let right = [piece[2] - cap, piece[1], piece[2], piece[3]];
        let c = [rgba(0xffffff, alpha); 4];
        f.draws.push((
            atlas,
            vec![
                quad([x, y, cap_w, h], left, c, 0),
                quad([x + cap_w, y, (w - 2. * cap_w).max(0.), h], mid, c, 0),
                quad([x + w - cap_w, y, cap_w, h], right, c, 0),
            ],
        ));
    }

    /// The pointing hand, its fingertip at (x, y).
    pub fn pointer(&self, p: &Paint, f: &mut Ui2dFrame, [x, y]: [f32; 2]) {
        let Some((_, atlas)) = self.sheets else {
            engine::launcher::triangle(
                f,
                p,
                [[x - 22., y - 9.], [x - 22., y + 9.], [x, y]],
                rgba(0xf2f5fb, 1.),
            );
            return;
        };
        let (w, h) = (40., 27.);
        f.draws.push((
            atlas,
            vec![quad(
                [x - w, y - h / 2., w, h],
                POINTER,
                [rgba(0xffffff, 1.); 4],
                0,
            )],
        ));
    }
}

fn read(root: &Path) -> Result<Read, String> {
    let image = root.join("Image");
    let hed = std::fs::read(image.join("SettingMenu.hed")).map_err(|e| e.to_string())?;
    let mut pkg = File::open(image.join("SettingMenu.pkg")).map_err(|e| e.to_string())?;
    let len = pkg.metadata().map_err(|e| e.to_string())?.len();
    let inventory = data::read_hed(&hed, len).map_err(|e| e.to_string())?;
    if inventory.entries.len() != ENTRIES {
        return Err(format!(
            "SettingMenu has {} entries, not {ENTRIES}",
            inventory.entries.len()
        ));
    }
    let mut payload = |at: usize| {
        data::original_payload::read_original_limited(
            &mut pkg,
            &inventory.entries[at],
            LAYOUT_LIMIT,
        )
        .map(|d| d.bytes)
        .map_err(|e| e.to_string())
    };
    let layout = payload(LAYOUT)?;
    if !layout.starts_with(b"L2D@") {
        return Err("the menu layout is not where it was".into());
    }
    let mut pictures = Vec::new();
    let mut at = 0;
    while pictures.len() < 2
        && let Some(found) = layout[at..].windows(4).position(|w| w == b"TIM2")
    {
        let o = at + found;
        pictures.push(bbs_data::bbs_map::tim2(&layout, o).map_err(|e| e.to_string())?);
        at = o + 4;
    }
    let [back, atlas] = <[_; 2]>::try_from(pictures).map_err(|_| "too few pictures")?;
    if [back.width, back.height] != [2048, 2048] || [atlas.width, atlas.height] != ATLAS_SIZE {
        return Err("the menu pictures are not the expected sizes".into());
    }
    let [bx, by, bw, bh] = BACKGROUND.map(|v| v as usize);
    let mut background = Vec::with_capacity(bw * bh * 4);
    for row in by..by + bh {
        let start = (row * back.width as usize + bx) * 4;
        background.extend_from_slice(&back.rgba[start..start + bw * 4]);
    }
    let bank_bytes = payload(SOUND_BANK)?;
    let sounds = match data::scd::read(&bank_bytes) {
        Ok(bank) => (0..bank.sound_count().min(16))
            .map(|i| {
                let sound = bank.sound(i).ok()??;
                let pcm = bank.decode(*sound.audio.first()? as usize).ok()??;
                Some(audio::Clip {
                    channels: pcm.channels,
                    rate: pcm.rate,
                    samples: pcm.samples.into(),
                    loop_frames: None,
                })
            })
            .collect(),
        Err(e) => {
            eprintln!("Menu sounds unreadable: {e}");
            Vec::new()
        }
    };
    Ok(Read {
        background,
        atlas: atlas.rgba,
        sounds,
    })
}
