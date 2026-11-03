//! The global screen fade dk::Fade (DAT_140abb3b0): in FUN_140156c90, out
//! FUN_140156ce0, rate FUN_140156d00, per-frame FUN_140156f10 in its task at
//! priority 330000 (after every 2D sprite, so it draws on top). Alpha runs
//! 0..0x80 (0x80 = opaque); black blends normally, white additively.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// Holding opaque for the in-delay frame.
    InDelay,
    In,
    Out,
    /// Opaque, held until the next fade.
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub state: State,
    pub white: bool,
    /// GS alpha byte, 0..0x80.
    pub alpha: u8,
    delay: i32,
    duration: f32,
    step: f32,
}

impl Default for Fade {
    fn default() -> Self {
        Self {
            state: State::Idle,
            white: false,
            alpha: 0,
            delay: 0,
            duration: 0.,
            step: 0.,
        }
    }
}

impl Fade {
    /// FUN_140156d00: 2n frames at 128 / 2n per frame; n = 0 is 2 frames
    /// at 1.0.
    fn setup(&mut self, n: u32) {
        if n != 0 {
            self.step = 128.0f32 / (2 * n) as f32;
            self.duration = (2 * n) as f32;
        } else {
            self.step = 1.;
            self.duration = 2.;
        }
    }

    pub fn fade_in(&mut self, n: u32, white: bool) {
        self.white = white;
        self.state = State::InDelay;
        self.delay = 1;
        self.setup(n);
    }

    pub fn fade_out(&mut self, n: u32, white: bool) {
        self.white = white;
        self.state = State::Out;
        self.setup(n);
    }

    /// FUN_140156f10 with frame step `dt`. Returns whether the fade draws
    /// this frame.
    pub fn update(&mut self, dt: f32) -> bool {
        match self.state {
            State::Idle => return false,
            State::InDelay => {
                if self.delay == 0 {
                    self.state = State::In;
                }
                self.delay -= 1;
                self.alpha = 0x80;
            }
            State::In => {
                self.duration -= dt;
                self.alpha = ((self.duration * self.step) as i32 as u8).min(0x80);
                if self.duration <= 0. {
                    self.alpha = 0;
                    self.state = State::Idle;
                }
            }
            State::Out => {
                self.duration -= dt;
                let b = ((self.duration * self.step) as i32 as u8).min(0x80);
                self.alpha = 0x80 - b;
                if self.duration <= 0. {
                    self.state = State::Held;
                    self.alpha = 0x80;
                }
            }
            State::Held => {}
        }
        true
    }

    /// RGBA multipliers for the fade quad (0x80 = 1.0) and whether it adds.
    pub fn colour(&self) -> ([f32; 4], bool) {
        let c = if self.white { 255. / 128. } else { 0. };
        ([c, c, c, self.alpha as f32 / 128.], self.white)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut f: Fade) -> Vec<u8> {
        let mut out = vec![];
        while f.update(1.) {
            out.push(f.alpha);
            if f.state == State::Held || out.len() > 500 {
                break;
            }
        }
        out
    }

    #[test]
    fn sequences_match_the_exe() {
        let mut f = Fade::default();
        f.fade_in(60, false);
        let a = run(f);
        assert_eq!(a.len(), 122);
        assert_eq!(&a[..4], &[128, 128, 126, 125]);
        assert_eq!(&a[118..], &[3, 2, 1, 0]);
        f.fade_out(60, false);
        let a = run(f);
        assert_eq!(a.len(), 120);
        assert_eq!(&a[..3], &[2, 3, 4]);
        assert_eq!(&a[117..], &[126, 127, 128]);
        for (n, fin, fout) in [
            (0, vec![128, 128, 1, 0], vec![127, 128]),
            (1, vec![128, 128, 64, 0], vec![64, 128]),
        ] {
            f.fade_in(n, false);
            assert_eq!(run(f), fin);
            f.fade_out(n, false);
            assert_eq!(run(f), fout);
        }
    }
}
