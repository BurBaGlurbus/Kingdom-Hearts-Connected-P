//! `--preload` (KHC-058): what is read on first use is read before play
//! instead, so nothing stalls a frame later. Sora stands and a line says so
//! until it is done: the arena, KH3's data (a duel that may use it), every
//! spell effect's models and textures, the match's art and cup sounds.
//! The launcher's Quick start asks whether to preload (its settings keep the answer).

use super::*;

/// [hub choice, KHC-058] Preloading gives up after this long and play starts.
const LIMIT_TICKS: u64 = 60 * 45;

#[derive(Resource, Default)]
pub struct Preload {
    pub on: bool,
    started: Option<u64>,
    pub done: bool,
    fx_warmed: bool,
    /// What is still loading, for the line on screen.
    pub waiting: Vec<&'static str>,
}

impl Preload {
    pub fn new(on: bool) -> Preload {
        Preload {
            on,
            ..Preload::default()
        }
    }
}

/// The match's art and cup sounds start at once when preloading (their
/// loader asks this instead of waiting for the lobby).
pub fn wanted(game: &Game) -> bool {
    game.ext.online().preloading
}

#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut preload: ResMut<Preload>,
    mirage: Option<Res<mirage_arena::MirageArena>>,
    kh3: Option<Res<kh3_model::Kh3Model>>,
    duel_match: Option<Res<duel_match_room::MatchRoom>>,
    fx: Option<ResMut<spell_fx::SpellFx>>,
    mut images: ResMut<Assets<Image>>,
    mut game: ResMut<Game>,
) {
    let p = &mut *preload;
    if !p.on || p.done {
        return;
    }
    let game = &mut *game;
    let tick = game.tick;
    let started = *p.started.get_or_insert_with(|| {
        game.ext.online_mut().preloading = true;
        game.event(serde_json::json!({"event":"preload_start","tick":tick}));
        tick
    });
    // Every spell effect's models and textures, all at once.
    if !p.fx_warmed
        && let Some(mut fx) = fx
    {
        let paxes: Vec<Arc<data::object_effect::Pax>> = game
            .magic
            .iter()
            .flat_map(|m| m.src.paxes.values().cloned())
            .collect();
        let n = fx.warm(&paxes, &mut images, usize::MAX);
        p.fx_warmed = true;
        game.event(serde_json::json!({"event":"preload_effects","tick":tick,"files":n}));
    }
    let mut waiting = Vec::new();
    let o = game.ext.online();
    if mirage.is_some() && game.versus && o.mirage == mirage_arena::Avail::Unknown {
        waiting.push("the arena");
    }
    if game.versus
        && o.kh3_install.is_some()
        && kh3
            .as_ref()
            .is_some_and(|k| k.loaded.is_none() && k.error.is_none())
    {
        waiting.push("KH3");
    }
    if duel_match.as_ref().is_some_and(|m| !m.preloaded()) {
        waiting.push("the match's art");
    }
    let timed_out = tick.saturating_sub(started) > LIMIT_TICKS;
    if tick.saturating_sub(started) % 300 == 299 {
        game.event(serde_json::json!({"event":"preload_waiting","tick":tick,"for":waiting}));
    }
    if waiting.is_empty() || timed_out {
        p.done = true;
        game.ext.online_mut().preloading = false;
        game.event(serde_json::json!({"event":"preload_done","tick":tick,
            "ticks":tick.saturating_sub(started),"timed_out":timed_out,"left":waiting}));
        waiting.clear();
    }
    p.waiting = waiting;
}

#[derive(Resource, Default)]
pub struct PreloadSheets(Option<(usize, usize)>);

/// "Preloading" and what it waits for, in the middle of the screen.
pub fn present(
    preload: Res<Preload>,
    hud: Option<Res<Hud>>,
    mut sheets: ResMut<PreloadSheets>,
    mut frame: ResMut<Ui2dFrame>,
    mut images: ResMut<Assets<Image>>,
) {
    if !preload.on || preload.done || preload.started.is_none() {
        return;
    }
    let Some(font) = hud.as_ref().and_then(|h| h.menu.as_ref()).map(|m| &m.font) else {
        return;
    };
    let (sheet, white) = *sheets.0.get_or_insert_with(|| {
        (
            frame.add_sheet(
                &mut images,
                font.rgba.clone(),
                font.size,
                font.texel_scale as f32,
            ),
            frame.add_sheet(&mut images, vec![255; 16], [2, 2], 1.),
        )
    });
    let f = field_command_menu::Font {
        widths: &font.widths,
        pad: 1,
    };
    let line = if preload.waiting.is_empty() {
        "Preloading".to_owned()
    } else {
        format!("Preloading {}", preload.waiting.join(", "))
    };
    let enc = pause_menu::encode_text(&line);
    let size = 14.;
    let w = field_command_menu::text_width(&enc, &f, size);
    let (x, y) = (256. - w / 2., 190.);
    let back = data::sprite_sequence::Quad {
        corners: [
            [x - 10., y - 5.],
            [x + w + 10., y - 5.],
            [x - 10., y + size + 5.],
            [x + w + 10., y + size + 5.],
        ],
        uv: [0, 0, 1, 1],
        colors: [[0., 0., 0., 0.6]; 4],
        blend: 0,
    };
    frame.draws.push((white, vec![back]));
    frame.draws.push((
        sheet,
        field_command_menu::text_quads_sized(&enc, [x, y], [1.6, 1.6, 1.6, 1.], &f, size),
    ));
}
