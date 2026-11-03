//! `SeqTextureAnim` (0x19): the model texture patches of the TEXA chunks
//! (private screen-spec 5). A command `no` sets, on every texture node
//! whose id map holds it, the idle animation; a node plays its idle
//! animation from the command's frame, one tick = half an event frame.
//! Nodes no command reaches play the model's default animation.
use bevy::prelude::*;
use data::event_script::Command;
use std::collections::HashMap;

/// Ticks per event frame (the exe runs events at dt 2).
const TICKS_PER_FRAME: f32 = 2.;

struct PatchState {
    /// Index into the model texture set's `play` list.
    play: usize,
    image: Handle<Image>,
    texture_width: u32,
    shown: Option<u16>,
}

#[derive(Resource)]
pub struct Texa {
    /// Per actor index: its patches.
    actors: Vec<Vec<PatchState>>,
    /// Per actor id: `(frame, number, flag)` in script order.
    commands: HashMap<i16, Vec<(i32, i16, u8)>>,
}

impl Texa {
    /// `shared` maps (actor index, environment ordinal) to the model
    /// texture's shared image handle.
    pub fn new(
        actors: &[super::ActorRuntime],
        shared: &HashMap<(usize, usize), (Handle<Image>, u32)>,
        script: &[Command],
    ) -> Self {
        let mut commands: HashMap<i16, Vec<(i32, i16, u8)>> = HashMap::new();
        for c in script {
            if let Command::TextureAnimation {
                frame,
                actor,
                number,
                flag,
                ..
            } = c
            {
                commands
                    .entry(*actor)
                    .or_default()
                    .push((*frame as i32, *number, *flag));
            }
        }
        for list in commands.values_mut() {
            list.sort_by_key(|c| c.0);
        }
        let actors = actors
            .iter()
            .enumerate()
            .map(|(ai, actor)| {
                let Some(set) = &actor.model.textures else {
                    return Vec::new();
                };
                set.play
                    .iter()
                    .enumerate()
                    .filter_map(|(play, p)| {
                        let (image, texture_width) = shared.get(&(ai, p.patch.environment))?;
                        Some(PatchState {
                            play,
                            image: image.clone(),
                            texture_width: *texture_width,
                            shown: None,
                        })
                    })
                    .collect()
            })
            .collect();
        Texa { actors, commands }
    }
}

/// Repaints every patch whose image changed at this event frame.
pub fn animate(
    mut event: ResMut<super::Event>,
    mut texa: ResMut<Texa>,
    mut images: ResMut<Assets<Image>>,
) {
    let frame = event.frame;
    let Texa { actors, commands } = &mut *texa;
    let mut log = Vec::new();
    for (ai, patches) in actors.iter_mut().enumerate() {
        let actor = &event.actors[ai];
        let Some(set) = &actor.model.textures else {
            continue;
        };
        let list = commands.get(&actor.id).map(Vec::as_slice).unwrap_or(&[]);
        for state in patches {
            let play = &set.play[state.play];
            let patch = &play.patch;
            let mut selected = (0, patch.default_animation);
            for &(at, number, flag) in list.iter().filter(|c| c.0 as f32 <= frame) {
                if flag == 1 {
                    if let Some(a) = u16::try_from(number)
                        .ok()
                        .and_then(|n| patch.animation_for(n))
                    {
                        selected = (at, Some(a));
                    }
                } else {
                    selected = (at, patch.default_animation);
                }
            }
            let Some(animation) = selected.1 else {
                continue;
            };
            let ticks = (frame - selected.0 as f32) * TICKS_PER_FRAME;
            let Some(image) = patch.image_at(animation, ticks) else {
                continue;
            };
            if state.shown == Some(image) {
                continue;
            }
            state.shown = Some(image);
            log.push(
                serde_json::json!({"event":"texture_anim","frame":frame,"actor":actor.id,
                "environment":patch.environment,"animation":animation,"image":image}),
            );
            if let Some(mut target) = images.get_mut(&state.image)
                && let Some(data) = target.data.as_mut()
            {
                patch.paint(data, state.texture_width, image, &play.clut_rgba);
            }
        }
    }
    event.log.extend(log);
}
