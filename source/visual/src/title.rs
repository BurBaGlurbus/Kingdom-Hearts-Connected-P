//! The title screen (overlay task FUN_1402b4590, menu frame FUN_1402b35a0,
//! leave FUN_1402b3c10, cursor layout FUN_1402b4480 / FUN_1402b3d20): the
//! main layout plays the start fade, intro and idle scenes of title.2ld; once
//! idle, a cursor layout shows the rows. Decisions hand off to the New Game
//! dialog, the file menu, the Theater or the collection launcher.
use data::menu_layout::{Layout, LayoutTask};

/// Logical pad bits read by the title (pad mode 7, FUN_14039c720).
pub mod bits {
    pub const DECIDE: u64 = 0x4;
    pub const CANCEL: u64 = 0x8;
    pub const UP: u64 = 0x10;
    pub const DOWN: u64 = 0x20;
    pub const START: u64 = 0x400;
}

pub const SCENE_BOOT: usize = 8;
pub const SCENE_START: usize = 17;
pub const SCENE_INTRO: [usize; 2] = [12, 13];
pub const SCENE_IDLE: usize = 14;
pub const SCENE_EXIT: [usize; 2] = [15, 16];
/// Cursor scenes per row: 3-row (0x1405b8620) and 4-row (0x1405b8624).
pub const CURSOR_SCENES: [&[usize]; 2] = [&[0, 1, 18], &[2, 3, 4, 19]];
/// Frames without decide/up/down before the attract demo.
pub const IDLE_LIMIT: f32 = 5100.;
/// FUN_1402eb740 requests and their se000 sounds: 7 cursor (#1), 8
/// decide (#2), 0xe New Game start (#21).
pub const SOUND_CURSOR: u32 = 1;
pub const SOUND_DECIDE: u32 = 2;
pub const SOUND_NEW_GAME: u32 = 21;
/// BGM fade-out (FUN_1405373e0(0x2d0)) and screen fade (0x3c).
pub const BGM_FADE: u32 = 720;
pub const SCREEN_FADE: u32 = 0x3c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewGame,
    Load,
    Theater,
    Demo,
    Back,
}

/// Sub-state DAT_140b1d5ec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Menu,
    NewGameDialog,
    Theater,
    Back,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// se000 sound number.
    Sound(u32),
    /// Start `vagstream/Title.win32.scd` unless a stream already plays.
    StartBgm,
    /// Fade the BGM out (FUN_1405373e0 units).
    FadeBgm(u32),
    /// Screen fade to black over n (FUN_140156ce0).
    FadeOut(u32),
    /// The file menu (menu 3) opens over the title.
    OpenFileMenu,
    /// The title has decided; play out and hand off.
    Leave(Action),
}

pub struct Title {
    pub main: LayoutTask,
    pub cursor_layout: Option<LayoutTask>,
    pub cursor: usize,
    pub prev_cursor: Option<usize>,
    /// Theater unlocked (DAT_140b0d54d): 4 rows.
    pub theater: bool,
    pub action: Option<Action>,
    pub state: State,
    pub timer: f32,
    pub leaving: bool,
}

impl Title {
    /// Task start (FUN_1402b4590): the main layout on the start scene (or
    /// the boot splash on a process's first title) with the intro queued.
    pub fn new(layout: &Layout, first_boot: bool, theater: bool) -> Self {
        let mut main = LayoutTask::new(layout, if first_boot { SCENE_BOOT } else { SCENE_START });
        main.next = Some(SCENE_INTRO[theater as usize]);
        Self {
            main,
            cursor_layout: None,
            cursor: 0,
            prev_cursor: None,
            theater,
            action: None,
            state: State::Menu,
            timer: 0.,
            leaving: false,
        }
    }

    pub fn count(&self) -> usize {
        3 + self.theater as usize
    }

    fn cursor_scene(&self) -> usize {
        CURSOR_SCENES[self.theater as usize][self.cursor]
    }

    /// FUN_1402b4480: the row layout on the cursor's scene.
    fn create_cursor_layout(&mut self, layout: &Layout) {
        let mut task = LayoutTask::new(layout, self.cursor_scene());
        task.next = None;
        self.cursor_layout = Some(task);
    }

    /// One title-task frame with this frame's new presses (`trig`).
    pub fn frame(&mut self, layout: &Layout, trig: u64, step: f32) -> Vec<Event> {
        let mut events = Vec::new();
        if self.leaving {
            return events;
        }
        match self.state {
            State::Menu => {
                if self.menu_frame(layout, trig, step, &mut events) {
                    self.leaving = true;
                } else if self.main.scene == SCENE_IDLE
                    && self.prev_cursor != Some(self.cursor)
                    && let Some(c) = self.cursor_layout.as_mut()
                {
                    c.set_scene(CURSOR_SCENES[self.theater as usize][self.cursor], None);
                }
            }
            // The dialog / Theater / Back hand-offs are driven by the caller.
            State::NewGameDialog | State::Theater | State::Back => {}
        }
        events
    }

    /// FUN_1402b35a0; true when the title leaves.
    fn menu_frame(&mut self, layout: &Layout, trig: u64, step: f32, ev: &mut Vec<Event>) -> bool {
        if trig & 0x34 == 0 {
            self.timer += step;
            if self.timer as i32 >= IDLE_LIMIT as i32 {
                self.timer = IDLE_LIMIT;
            }
        } else {
            self.timer = 0.;
        }
        if self.cursor_layout.is_none() {
            let intro = SCENE_INTRO.contains(&self.main.scene);
            if !self.main.just_switched {
                if intro && trig & (bits::DECIDE | bits::START) != 0 {
                    self.main.set_scene(SCENE_IDLE, None);
                    self.create_cursor_layout(layout);
                }
            } else if !intro {
                self.create_cursor_layout(layout);
                self.main.next = None;
            } else {
                self.main.next = Some(SCENE_IDLE);
                ev.push(Event::StartBgm);
            }
            return false;
        }
        self.prev_cursor = Some(self.cursor);
        let count = self.count();
        let mut leave = false;
        if trig & bits::UP != 0 {
            self.cursor = if self.cursor == 0 {
                count - 1
            } else {
                self.cursor - 1
            };
        } else if trig & bits::DOWN != 0 {
            self.cursor = if self.cursor + 1 >= count {
                0
            } else {
                self.cursor + 1
            };
        } else if trig & bits::DECIDE != 0 {
            let rows: &[Action] = if self.theater {
                &[Action::NewGame, Action::Load, Action::Theater, Action::Back]
            } else {
                &[Action::NewGame, Action::Load, Action::Back]
            };
            let action = rows[self.cursor];
            self.action = Some(action);
            match action {
                Action::NewGame => {
                    self.state = State::NewGameDialog;
                    ev.push(Event::Sound(SOUND_DECIDE));
                }
                Action::Load => ev.push(Event::OpenFileMenu),
                Action::Theater => {
                    self.state = State::Theater;
                    ev.push(Event::Sound(SOUND_DECIDE));
                }
                Action::Back => {
                    ev.push(Event::FadeBgm(BGM_FADE));
                    self.state = State::Back;
                    ev.push(Event::FadeOut(SCREEN_FADE));
                    ev.push(Event::Sound(SOUND_DECIDE));
                }
                Action::Demo => {}
            }
        } else if self.timer as i32 >= IDLE_LIMIT as i32 {
            self.action = Some(Action::Demo);
            leave = true;
        }
        if self.prev_cursor != Some(self.cursor) {
            ev.push(Event::Sound(SOUND_CURSOR));
            let scene = self.cursor_scene();
            if let Some(c) = self.cursor_layout.as_mut() {
                c.set_scene(scene, None);
            }
        }
        if leave {
            self.leave(ev);
        }
        leave
    }

    /// FUN_1402b3c10: the rows go; the main layout plays its exit scene
    /// (none for Load; the demo fades out instead) and the BGM fades.
    pub fn leave(&mut self, ev: &mut Vec<Event>) {
        if let Some(c) = self.cursor_layout.as_mut() {
            c.kill();
        }
        match self.action {
            Some(Action::Load) => self.main.kill(),
            Some(Action::Demo) => ev.push(Event::FadeOut(SCREEN_FADE)),
            _ => self.main.set_scene(SCENE_EXIT[self.theater as usize], None),
        }
        ev.push(Event::FadeBgm(BGM_FADE));
        if self.action == Some(Action::NewGame) {
            ev.push(Event::Sound(SOUND_NEW_GAME));
        }
        if let Some(a) = self.action {
            ev.push(Event::Leave(a));
        }
        self.leaving = true;
    }

    /// The New Game dialog's result (FUN_1402b7420): confirmed leaves with
    /// the exit scene; cancelled returns to the menu with the timer reset.
    pub fn new_game_result(&mut self, confirmed: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        if confirmed {
            self.action = Some(Action::NewGame);
            self.leave(&mut ev);
        } else {
            self.state = State::Menu;
            self.timer = 0.;
        }
        ev
    }

    /// Advances both layout tasks one frame (their own tasks).
    pub fn update_layouts(&mut self, layout: &Layout, step: f32) {
        self.main.update(layout, step);
        if let Some(c) = self.cursor_layout.as_mut() {
            c.update(layout, step);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data::menu_layout::{Prop, Scene};
    use data::sprite_sequence::{Animation, AnimationGroup, Sequence};

    /// Twenty scenes, each one prop on a group lasting `len` frames; the
    /// idle scene (14) loops.
    fn layout(len: i32) -> Layout {
        let animation = |end| Animation {
            flags: data::sprite_sequence::LINEAR,
            sprite_group: 0,
            frames: [0, end],
            translate: [0; 4],
            pivot: [0; 4],
            rotation: [0.; 6],
            scale: [1.; 2],
            scale_x: [1.; 2],
            scale_y: [1.; 2],
            curve: [0.; 4],
            bounce: [0.; 4],
            bounce_count: [0; 2],
            blend: 0,
            color: [0x8080_8080; 2],
        };
        let group = |plays| AnimationGroup {
            animations: 0..1,
            play_count: plays,
            loop_frames: [0, 0],
            light_position_x: 0,
            text_position_y: 0,
            text_scale: 0,
            ui_padding: 0,
            text_position_x: 0,
        };
        let sequence = Sequence {
            sprites: vec![],
            parts: vec![],
            sprite_groups: std::iter::once(0..0).collect(),
            animations: vec![animation(len - 1)],
            groups: vec![group(1), group(0)],
        };
        Layout {
            props: (0..20)
                .map(|s| Prop {
                    texture: 0,
                    sequence: 0,
                    group: (s == SCENE_IDLE) as usize,
                    show_at: 0,
                    position: [0, 0],
                })
                .collect(),
            scenes: (0..20)
                .map(|s| Scene {
                    props: s..s + 1,
                    max_frame: 0,
                })
                .collect(),
            sequences: vec![sequence],
        }
    }

    /// Layout tasks run before the title task each frame.
    fn run(t: &mut Title, l: &Layout, trig: u64) -> Vec<Event> {
        t.update_layouts(l, 1.);
        t.frame(l, trig, 1.)
    }

    #[test]
    fn start_intro_idle_then_rows() {
        let l = layout(10);
        let mut t = Title::new(&l, false, false);
        let mut bgm = 0;
        for _ in 0..40 {
            bgm += run(&mut t, &l, 0)
                .iter()
                .filter(|e| **e == Event::StartBgm)
                .count();
        }
        assert_eq!(bgm, 1);
        assert_eq!(t.main.scene, SCENE_IDLE);
        assert_eq!(t.cursor_layout.as_ref().unwrap().scene, 0);
    }

    #[test]
    fn decide_skips_the_intro_only() {
        let l = layout(100);
        let mut t = Title::new(&l, false, false);
        run(&mut t, &l, bits::DECIDE);
        assert_eq!(t.main.scene, SCENE_START);
        while t.main.scene == SCENE_START {
            run(&mut t, &l, 0);
        }
        run(&mut t, &l, 0);
        run(&mut t, &l, bits::START);
        assert_eq!(t.main.scene, SCENE_IDLE);
        assert!(t.cursor_layout.is_some());
    }

    #[test]
    fn cursor_wraps_with_sounds() {
        let l = layout(5);
        let mut t = Title::new(&l, false, true);
        while t.cursor_layout.is_none() {
            run(&mut t, &l, 0);
        }
        assert_eq!(run(&mut t, &l, bits::UP), vec![Event::Sound(SOUND_CURSOR)]);
        assert_eq!(t.cursor, 3);
        run(&mut t, &l, 0);
        assert_eq!(t.cursor_layout.as_ref().unwrap().scene, 19);
        run(&mut t, &l, bits::DOWN);
        assert_eq!(t.cursor, 0);
        assert_eq!(
            run(&mut t, &l, bits::DECIDE),
            vec![Event::Sound(SOUND_DECIDE)]
        );
        assert_eq!(t.state, State::NewGameDialog);
        let ev = t.new_game_result(true);
        assert_eq!(
            ev,
            vec![
                Event::FadeBgm(BGM_FADE),
                Event::Sound(SOUND_NEW_GAME),
                Event::Leave(Action::NewGame)
            ]
        );
        assert_eq!(t.main.scene, SCENE_EXIT[1]);
    }

    /// First launch (FUN_14014ed40 sets the first-boot bit once per
    /// process): scene 8 ignores decide/START and hands straight to the
    /// intro, without the scene-17 fade.
    #[test]
    fn boot_splash_is_unskippable_and_leads_to_the_intro() {
        let l = layout(30);
        let mut t = Title::new(&l, true, false);
        assert_eq!(t.main.scene, SCENE_BOOT);
        let mut scenes = vec![t.main.scene];
        for _ in 0..40 {
            run(&mut t, &l, bits::DECIDE | bits::START);
            if scenes.last() != Some(&t.main.scene) {
                scenes.push(t.main.scene);
            }
            if t.main.scene != SCENE_BOOT {
                break;
            }
        }
        assert_eq!(scenes, vec![SCENE_BOOT, SCENE_INTRO[0]]);
    }

    #[test]
    fn idle_demo_after_5100_frames() {
        let l = layout(5);
        let mut t = Title::new(&l, false, false);
        let mut frames = 0;
        loop {
            frames += 1;
            if run(&mut t, &l, 0).contains(&Event::Leave(Action::Demo)) {
                break;
            }
        }
        assert_eq!(frames, 5100);
    }
}
