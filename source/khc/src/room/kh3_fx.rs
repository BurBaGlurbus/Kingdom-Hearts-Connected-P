//! KH3's effects and sounds in the duel [T282]: the clips' own notifies (attach
//! effect, weapon swing, voice, sound), the Cascade evaluator (`effect::cascade`,
//! ported from StrawHatPro's kh-vfx, MIT) and SEAD banks from the player's paks.
//! Everything here is presentation: each PC replays it from the Sora's `Visual`,
//! the same clip and clock on both, so the wire carries only ids and hit events.
//!
//! Materials are approximated: a texture, its opacity and a tint, from the
//! parameters that survive cooking (the reference's generic path), blended as
//! the rest of the room's effects are, on stored sRGB values.

#![allow(clippy::type_complexity, clippy::chunks_exact_to_as_chunks)]

use super::*;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh3_cascade::{Blend, Camera, DrawList, EffectDef, EffectId, SpawnOptions, World, Xform};
use kh3_data::anim::BonePose;
use kh3_data::material::{self, Blend as MatBlend, MaterialParams};
use kh3_data::props::Props;
use kh3_data::sead::{self, Banks, Keys, Kind as SoundKind};
use kh3_data::sora::{self, SoraData};
use kh3_data::static_mesh::StaticMesh;
use kh3_data::texture::Texture;
use kh3_data::{Paks, effect_json};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use visual::effect_draw::{Blend as DrawBlend, EffectMaterial};

const KEY_BASE: u32 = 0x4b33_0000;
const SWING_BANK: &str = "/Game/Sound/SE/weapon/02001";
const FOOT_TABLE: &str = "/Game/Sound/SE/_Data/MASTS/FsTable_Sora";
const FOOT_SETTING: &str = "/Game/Sound/SE/_Data/MASTS/DetectioinSetting_Sora";
const TRAIL: &str = "/Game/Effects/wep/w_so010/par/VW_SO010_LOC0";

/// A material as the renderer draws it.
#[derive(Clone, Debug)]
pub struct FxMaterial {
    pub texture: Option<String>,
    pub opacity: Option<String>,
    pub mono: bool,
    pub blend: MatBlend,
    pub tint: [f32; 3],
    /// No texture at all: a mesh with it draws in plain colour.
    pub flat: bool,
    /// Its texture's channels are masks: the red one, as grey.
    pub packed: bool,
    /// A mesh with it shows by the rim.
    pub rim: bool,
}

/// One row of an attach-effect notify.
#[derive(Clone, Debug)]
pub struct EffectRow {
    pub effect: String,
    pub socket: String,
    pub on_weapon: bool,
    pub attach: bool,
    pub location: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub fade_in: f32,
    pub delay: f32,
    pub color: [f32; 4],
    /// Stops when the animation changes (the usual `WhenChangeAnimationType`).
    pub ends_with_clip: bool,
    pub state: bool,
}

#[derive(Clone, Debug)]
pub enum Cue {
    Effect(EffectRow),
    Trail,
    Sound {
        path: String,
        volume: f32,
        pitch: f32,
    },
    Voice(Vec<(String, u32)>),
    /// A sound held for the window, its end sound after it.
    Held {
        path: String,
        end: Option<String>,
    },
    /// A table entry of the auto SE for the current surface (`SQEXSEADAnimNotify(State)_PlayAutoSe`).
    AutoSe {
        motion: kh3_autose::Motion,
        state: bool,
    },
    /// The auto SE's detectors for the window (`SQEXSEADAnimNotifyState_ChangeAutoSeEnable`).
    AutoEnable(bool),
    /// An attach-data asset's rows; replaced by `Effect`s when the asset is read.
    Rows {
        asset: String,
        group: i64,
        socket: String,
        on_weapon: bool,
        state: bool,
    },
}

/// `FsTable_Sora`: per surface the sounds of motion types 1 to 6, then the others from type 7.
#[derive(Clone, Debug, Default)]
pub struct AttachRows(pub HashMap<String, Vec<(i64, EffectRow)>>);

#[derive(Clone, Debug, Default)]
pub struct FootTable {
    pub per_surface: Vec<[Option<String>; 6]>,
    pub others: Vec<Option<String>>,
    pub settings: kh3_autose::Settings,
}

impl FootTable {
    fn sound(&self, surface: usize, motion: kh3_autose::Motion) -> Option<&String> {
        let t = motion as usize;
        if t <= 6 {
            self.per_surface.get(surface)?.get(t - 1)?.as_ref()
        } else {
            self.others.get(t - 7)?.as_ref()
        }
    }

    fn read(paks: &Paks) -> Option<FootTable> {
        let p = paks.game_package(FOOT_TABLE).ok()??;
        let (props, _) = kh3_data::props::export_props(&p, 0).ok()?;
        let path = |v: &kh3_data::props::Value| {
            v.as_str()
                .filter(|s| !s.is_empty() && *s != "None")
                .map(|s| s.split('.').next().unwrap_or(s).to_owned())
        };
        let mut table = FootTable::default();
        for q in props.all("PerSurfaceInfos") {
            let Some(row) = q.value.as_props() else {
                continue;
            };
            let at = q.index.max(0) as usize;
            if table.per_surface.len() <= at {
                table.per_surface.resize(at + 1, Default::default());
            }
            for s in row.all("SoundAssetPaths") {
                if let Some(slot) = table.per_surface[at].get_mut(s.index.max(0) as usize) {
                    *slot = path(&s.value);
                }
            }
        }
        if let Some(row) = props.props("NonSurfaceInfos") {
            for s in row.all("SoundAssetPaths") {
                let at = s.index.max(0) as usize;
                if table.others.len() <= at {
                    table.others.resize(at + 1, None);
                }
                table.others[at] = path(&s.value);
            }
        }
        if let Some(p) = paks.game_package(FOOT_SETTING).ok().flatten()
            && let Ok((props, _)) = kh3_data::props::export_props(&p, 0)
            && let Some(foot) = props.props("DetectorSettingFootStep")
        {
            if let Some(v) = foot.f32("FootShuffleAssumeBodyStopVelocityThreshold") {
                table.settings.shuffle_body_velocity = v;
            }
            if let Some(v) = foot.f32("FootShuffleDetectFootAccelThreshold") {
                table.settings.shuffle_foot_accel = v;
            }
        }
        Some(table)
    }
}

#[derive(Clone, Debug)]
pub struct Trigger {
    pub start: f32,
    pub end: f32,
    pub cue: Cue,
}

/// What the load thread hands over.
pub struct FxData {
    pub defs: Vec<EffectDef>,
    pub materials: Vec<(String, FxMaterial)>,
    pub meshes: Vec<(String, StaticMesh)>,
    pub triggers: HashMap<String, Vec<Trigger>>,
    pub textures: HashMap<String, Texture>,
    pub sounds: SoundBook,
    /// Effect name to the sounds its SeAttach data plays.
    pub effect_sounds: HashMap<String, Vec<String>>,
    pub feet: FootTable,
    /// `ead_*` by name: each row's group and placement.
    pub ead: AttachRows,
    /// Materials with their translated shaders, by name, and the shaders.
    pub shaded: HashMap<String, kh3_shaded::Record>,
    pub variants: Vec<kh3_shaded::Variant>,
}

/// Decoded sounds by their asset.
#[derive(Default)]
pub struct SoundBook {
    pub sounds: HashMap<String, Arc<Playable>>,
}

/// A resolved sound: its notes, with clips decoded.
pub struct Playable {
    pub kind: SoundKind,
    pub volume: f32,
    pub asset_volume: f32,
    pub spatial: bool,
    pub window: f32,
    pub sequences: Vec<(u8, u8, Vec<PlayNote>)>,
    pub cursor: AtomicUsize,
}

pub struct PlayNote {
    pub delay: f32,
    pub clip: audio::Clip,
    pub volume: f32,
    pub volume_random: Option<(f32, f32, u8)>,
    pub pitch_random: Option<(f32, f32, u8)>,
}

fn vector(p: &Props, k: &str, or: [f32; 3]) -> [f32; 3] {
    p.floats(k)
        .filter(|v| v.len() == 3)
        .map_or(or, |v| [v[0], v[1], v[2]])
}

fn english(path: &str) -> String {
    path.replace("/Localization/ja/", "/Localization/en/")
}

/// One placement row (`SQEX_StructParticleAttachData`), `object` resolving its effects' references: its (group, row)s.
fn placement(
    row: &Props,
    object: &dyn Fn(i32) -> Option<String>,
    list_socket: &str,
    on_weapon: bool,
    state: bool,
) -> Vec<(i64, EffectRow)> {
    let mut out = Vec::new();
    let socket = row
        .str("SocketName")
        .filter(|s| *s != "None")
        .unwrap_or(list_socket);
    for e in row.array("EffectDataList").unwrap_or(&[]) {
        let Some(e) = e.as_props() else { continue };
        let Some(path) = e.object("AttachEffect").and_then(object) else {
            continue;
        };
        let (c, a) = (
            row.floats("ParticleColor")
                .filter(|v| v.len() >= 3)
                .map_or([1.; 3], |v| [v[0], v[1], v[2]]),
            row.f32("ParticleAlpha").unwrap_or(1.),
        );
        out.push((
            e.int("GroupID").unwrap_or(0),
            EffectRow {
                effect: path,
                socket: socket.to_owned(),
                on_weapon,
                attach: row.bool("ToAttach").unwrap_or(true),
                location: vector(row, "LocationOffset", [0.; 3]),
                rotation: vector(row, "RotationOffset", [0.; 3]),
                scale: vector(row, "ScaleOffset", [1.; 3]),
                fade_in: row.f32("FadeIn").unwrap_or(0.),
                delay: row.f32("StartDelayTime").unwrap_or(0.),
                color: [c[0], c[1], c[2], a],
                ends_with_clip: !row
                    .str("WhenChangeAnimationType")
                    .is_some_and(|t| t.ends_with("NONE")),
                state,
            },
        ));
    }
    out
}

/// An attach-data asset's rows (`SQEX_ParticleAttachDataAsset`).
fn attach_rows(paks: &Paks, path: &str) -> Vec<(i64, EffectRow)> {
    let mut out = Vec::new();
    let Ok(Some(p)) = paks.game_package(path) else {
        return out;
    };
    let Some(i) = p.export_of_class("SQEX_ParticleAttachDataAsset") else {
        return out;
    };
    let Ok((props, _)) = kh3_data::props::export_props(&p, i) else {
        return out;
    };
    let object = |o: i32| p.import_package(o).map(str::to_owned);
    for row in props.array("EffectDataList").unwrap_or(&[]) {
        if let Some(row) = row.as_props() {
            out.extend(placement(row, &object, "", false, false));
        }
    }
    out
}

/// The rows of a notify's `MetaAttachDataList` and weapon lists.
fn effect_rows(n: &sora::Notify, out: &mut Vec<Trigger>) {
    if let Some(list) = n.props.array("AttachDataList") {
        for row in list {
            let Some(row) = row.as_props() else { continue };
            if row.bool("bUseCommonAttachData") == Some(true) {
                if row
                    .str("AttachType")
                    .is_some_and(|t| t.ends_with("ATTACH_TO_WEAPON"))
                {
                    out.push(Trigger {
                        start: n.start,
                        end: n.end,
                        cue: Cue::Trail,
                    });
                }
                continue;
            }
            // Assets of placement rows.
            let on_weapon = row.str("AttachType").is_some_and(|t| t.contains("WEAPON"));
            for asset in row.array("AttachDatas").unwrap_or(&[]) {
                let Some(path) = asset.as_object().and_then(|o| n.imports.get(&o)) else {
                    continue;
                };
                out.push(Trigger {
                    start: n.start,
                    end: n.end,
                    cue: Cue::Rows {
                        asset: path.clone(),
                        group: row.int("GroupID").unwrap_or(-2),
                        socket: row.str("SocketName").unwrap_or("").to_owned(),
                        on_weapon,
                        state: n.state,
                    },
                });
            }
        }
    }
    for list in n.props.array("MetaAttachDataList").unwrap_or(&[]) {
        let Some(list) = list.as_props() else {
            continue;
        };
        let on_weapon = list.str("AttachType").is_some_and(|t| t.contains("WEAPON"));
        let list_socket = list.str("SocketName").unwrap_or("");
        let object = |o: i32| n.imports.get(&o).cloned();
        for row in list.array("MetaAttachData").unwrap_or(&[]) {
            let Some(row) = row.as_props() else { continue };
            for (group, placed) in placement(row, &object, list_socket, on_weapon, n.state) {
                if plays_for_kingdom_key(group, &placed.effect) {
                    out.push(Trigger {
                        start: n.start,
                        end: n.end,
                        cue: Cue::Effect(placed),
                    });
                }
            }
        }
    }
}

/// The plain Keyblade's group is 0; -1 plays for any.
fn plays_for_kingdom_key(group: i64, effect: &str) -> bool {
    let name = effect_json::base_name(effect).to_ascii_lowercase();
    (group == 0 || group == -1) && !(name.starts_with("vw_so") && !name.starts_with("vw_so010"))
}

fn trigger_of(n: &sora::Notify, out: &mut Vec<Trigger>) {
    let object = |p: &Props, k: &str| p.object(k).and_then(|o| n.imports.get(&o).cloned());
    match n.class.as_str() {
        "TresAnimNotify_AttachEffect" | "TresAnimNotifyState_AttachEffect" => effect_rows(n, out),
        "TresAnimNotify_PlayWeaponSwingSE" | "TresAnimNotifyState_PlayWeaponSwingSE" => {
            let Some(id) = n.props.str("m_SoundAliasID") else {
                return;
            };
            let Some(index) = id
                .rsplit("SWING")
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|i| *i <= 10)
            else {
                return;
            };
            out.push(Trigger {
                start: n.start,
                end: n.end,
                cue: Cue::Sound {
                    path: format!("{SWING_BANK}/se02001_{}", 100 + index),
                    volume: n.props.f32("m_VolumeMultiplier").unwrap_or(1.),
                    pitch: n.props.f32("m_PitchMultiplier").unwrap_or(1.),
                },
            });
        }
        "TresAnimNotify_PlayVoice" | "TresAnimNotifyState_PlayVoice" => {
            let mut list = Vec::new();
            if let Some(v) = object(&n.props, "m_VoiceAsset") {
                list.push((english(&v), 1));
            }
            for r in n.props.array("m_RandomVoiceAssets").unwrap_or(&[]) {
                let Some(r) = r.as_props() else { continue };
                if let Some(v) = object(r, "Voice") {
                    let w = r.int("RandomWeights").unwrap_or(1).max(1) as u32;
                    list.push((english(&v), w));
                }
            }
            if !list.is_empty() {
                out.push(Trigger {
                    start: n.start,
                    end: n.end,
                    cue: Cue::Voice(list),
                });
            }
        }
        "SQEXSEADAnimNotify_PlaySound" => {
            if let Some(path) = object(&n.props, "Sound") {
                out.push(Trigger {
                    start: n.start,
                    end: n.end,
                    cue: Cue::Sound {
                        path,
                        volume: n.props.f32("VolumeMultiplier").unwrap_or(1.),
                        pitch: n.props.f32("PitchMultiplier").unwrap_or(1.),
                    },
                });
            }
        }
        "SQEXSEADAnimNotify_PlayAutoSe" | "SQEXSEADAnimNotifyState_PlayAutoSe" => {
            let motion = n
                .props
                .props("PlaySettings")
                .and_then(|s| s.str("MotionSoundType"))
                .and_then(kh3_autose::Motion::named);
            if let Some(motion) = motion {
                out.push(Trigger {
                    start: n.start,
                    end: n.end,
                    cue: Cue::AutoSe {
                        motion,
                        state: n.state,
                    },
                });
            }
        }
        "SQEXSEADAnimNotifyState_ChangeAutoSeEnable" => out.push(Trigger {
            start: n.start,
            end: n.end,
            // The class default is off; only the glide loops' windows turn it on.
            cue: Cue::AutoEnable(n.props.0.iter().any(|q| q.value.as_bool() == Some(true))),
        }),
        "SQEXSEADAnimNotifyState_AttachSound" => {
            if let Some(path) = object(&n.props, "Sound") {
                out.push(Trigger {
                    start: n.start,
                    end: n.end,
                    cue: Cue::Held {
                        path,
                        end: object(&n.props, "LoopEndSound"),
                    },
                });
            }
        }
        _ => {}
    }
}

/// The name priority of the reference's generic material path.
fn pick_textures(m: &MaterialParams) -> (Option<String>, Option<String>) {
    const EMISSIVE: [&str; 10] = [
        "BaseColorTex",
        "Tex Emi",
        "Tex Main",
        "Tex Core",
        "Emi Tex",
        "texture",
        "base_tex",
        "tex_A",
        "Tex Glow",
        "1 Grayscale Texture A",
    ];
    const OPACITY: [&str; 6] = [
        "Tex Opa",
        "1 Tex Opa",
        "Tex Opq",
        "Tex Mask",
        "Tex_mask1",
        "constraint_tex",
    ];
    let emissive = EMISSIVE
        .iter()
        .find_map(|k| m.textures.get(*k).cloned())
        .or_else(|| {
            m.textures
                .iter()
                .filter(|(k, _)| !k.starts_with(material::SAMPLED))
                .find_map(|(k, v)| {
                    let l = k.to_ascii_lowercase();
                    let bad = [
                        "nml",
                        "normal",
                        "wpo",
                        "dist",
                        "break",
                        "constraint",
                        "opa",
                        "mask",
                    ];
                    (!bad.iter().any(|b| l.contains(b))).then(|| v.clone())
                })
        })
        .or_else(|| {
            // Textures the root samples with no parameter: which feeds the colour did not
            // survive cooking, so the material's own picture before the shared ones, and the
            // shared noise and gradients last.
            m.textures
                .iter()
                .filter_map(|(k, v)| {
                    let base = k.strip_prefix(material::SAMPLED)?.to_ascii_lowercase();
                    sampled_rank(&base).map(|rank| (rank, v))
                })
                .min_by_key(|(rank, _)| *rank)
                .map(|(_, v)| v.clone())
        });
    let opacity = OPACITY.iter().find_map(|k| m.textures.get(*k).cloned());
    (emissive, opacity)
}

/// How good a sampled texture is as a material's colour, smallest first; normal maps and masks
/// never are.
fn sampled_rank(base: &str) -> Option<u8> {
    if base.ends_with('n') || base.contains("msk") || base.contains("ic_bsc") {
        return None;
    }
    let shared = base.starts_with("jtx_");
    let noise = ["_dis_", "_grd", "check"].iter().any(|n| base.contains(n));
    Some(match (shared, noise) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    })
}

/// A material that only bends the scene behind it: this renderer has nothing to show for it.
fn only_bends(m: &MaterialParams) -> bool {
    let root = m.root.to_ascii_lowercase();
    ["_dis_", "_dst_", "blur", "dummy"]
        .iter()
        .any(|n| root.contains(n))
        || m.textures.contains_key("Dist Texture")
        // Aero's wind wall, which its shader scrolls and cuts to streaks; whole it is a solid wall.
        || m.textures.values().any(|t| t.contains("_mzl_") && t.ends_with("_clmp"))
        || m.scalars.contains_key("Refraction Far")
}

/// A root that shows a mesh by its rim, seen by the view angle: "fresnel" in its name, or the `f`
/// of its kind letters (`jmb_wht_fsa`, `tmt_tor_mix00fsa`; `s` soft, `a` translucent, `k` additive).
fn shows_rim(root: &str) -> bool {
    let root = root.to_ascii_lowercase();
    if root.contains("fres") || root.contains("_frn") {
        return true;
    }
    let kind: String = root
        .chars()
        .rev()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    kind.contains('f') && kind.chars().all(|c| "fsakml".contains(c))
}

/// The red channel of a shared texture: those pack masks in their channels, so their colours
/// are no picture.
fn gray_of(t: &Texture, limit: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mip = t
        .mips
        .iter()
        .find(|m| m.width.max(m.height) <= limit && !m.bytes.is_empty())
        .or(t.mips.last())?;
    let px = kh3_ui::gfx::decode_texture(&t.format, mip.width, mip.height, &mip.bytes)?;
    Some((
        mip.width,
        mip.height,
        px.chunks_exact(4).map(|c| c[0]).collect(),
    ))
}

fn fx_material(m: &MaterialParams) -> FxMaterial {
    let (texture, opacity) = if only_bends(m) {
        (None, None)
    } else {
        pick_textures(m)
    };
    let flat = texture.is_none() && opacity.is_none() && !only_bends(m);
    // A shared texture no parameter names, and the shared noise and shock pictures under any
    // name: their channels are masks.
    let packed = m.textures.iter().any(|(k, v)| {
        let file = v.rsplit('/').next().unwrap_or(v).to_ascii_lowercase();
        Some(v) == texture.as_ref()
            && file.starts_with("jtx_")
            && (k.starts_with(material::SAMPLED)
                || file.contains("_dis_")
                || file.contains("_sho_"))
    });
    let scalar = |k: &str| m.scalars.get(k).copied();
    let vector = |k: &str| m.vectors.get(k).copied();
    let mult = scalar("Bright Multi Emi").filter(|m| *m > 0.).unwrap_or(1.);
    let tint = vector("2c White Color")
        .or_else(|| vector("Emi Color"))
        .map_or([1.; 3], |c| [c[0], c[1], c[2]]);
    FxMaterial {
        texture,
        opacity,
        mono: false,
        blend: m.blend,
        tint: [tint[0] * mult, tint[1] * mult, tint[2] * mult],
        flat,
        packed,
        rim: shows_rim(&m.root),
    }
}

/// Reads the sounds, effects, materials and meshes the clips name.
pub fn load(
    paks: &Paks,
    data: &SoraData,
    book: &kh3_player::attack::Book,
    install: &kh3_data::Install,
) -> Result<FxData, String> {
    let mut triggers: HashMap<String, Vec<Trigger>> = HashMap::new();
    for clip in &data.anims.clips {
        let mut list = Vec::new();
        for n in &clip.notifies {
            trigger_of(n, &mut list);
        }
        if !list.is_empty() {
            triggers.insert(clip.name.clone(), list);
        }
    }
    // The attach-data assets: the notifies' and the core's.
    let mut ead = HashMap::new();
    for list in triggers.values_mut() {
        let mut expanded = Vec::with_capacity(list.len());
        for t in list.drain(..) {
            let Cue::Rows {
                asset,
                group,
                socket,
                on_weapon,
                state,
            } = &t.cue
            else {
                expanded.push(t);
                continue;
            };
            let name = effect_json::base_name(asset).to_ascii_lowercase();
            let rows: &Vec<(i64, EffectRow)> =
                ead.entry(name).or_insert_with(|| attach_rows(paks, asset));
            for (row_group, row) in rows {
                // -2 asks for every current group: on the plain Keyblade, 0.
                let wanted = if *group == -2 {
                    plays_for_kingdom_key(*row_group, &row.effect)
                } else {
                    *row_group == *group || *row_group == -1
                };
                if wanted {
                    let mut row = row.clone();
                    if row.socket.is_empty() {
                        row.socket.clone_from(socket);
                    }
                    row.on_weapon = *on_weapon;
                    row.state = *state;
                    expanded.push(Trigger {
                        start: t.start,
                        end: t.end,
                        cue: Cue::Effect(row),
                    });
                }
            }
        }
        *list = expanded;
    }
    for (name, path) in &book.by_name {
        if name.starts_with("ead_") {
            ead.insert(name.clone(), attach_rows(paks, path));
        }
    }
    let mut effects: Vec<String> = vec![TRAIL.to_owned()];
    effects.extend(ead.values().flatten().map(|(_, row)| row.effect.clone()));
    let mut sounds: Vec<String> = (1..=10)
        .map(|i| format!("{SWING_BANK}/se02001_{}", 100 + i))
        .chain((1..=7).map(|i| format!("{SWING_BANK}/se02001_00{i}")))
        .collect();
    let feet = FootTable::read(paks).ok_or("KH3's footstep table is missing")?;
    // The surfaces a duel has, and the sounds that have none.
    for surface in [0, 10, 11] {
        if let Some(row) = feet.per_surface.get(surface) {
            sounds.extend(row.iter().flatten().cloned());
        }
    }
    sounds.extend(feet.others.iter().flatten().cloned());
    sounds.extend(UI_SOUNDS.iter().map(|&(_, n)| ui_sound_path(n)));
    for list in triggers.values() {
        for t in list {
            match &t.cue {
                Cue::Effect(r) => effects.push(r.effect.clone()),
                Cue::Sound { path, .. } => sounds.push(path.clone()),
                Cue::Voice(v) => sounds.extend(v.iter().map(|x| x.0.clone())),
                Cue::Held { path, end, .. } => {
                    sounds.push(path.clone());
                    sounds.extend(end.clone());
                }
                Cue::Trail | Cue::AutoSe { .. } | Cue::AutoEnable(_) | Cue::Rows { .. } => {}
            }
        }
    }
    // The spells' effects and sounds.
    fn stage_assets(
        s: &kh3_player::spell::Stage,
        effects: &mut Vec<String>,
        sounds: &mut Vec<String>,
    ) {
        effects.extend(s.body_effect.clone());
        effects.extend(s.hit_effect.clone());
        effects.extend(s.map_hit_effect.clone());
        sounds.extend(s.hit_sound.clone());
        sounds.extend(s.map_hit_sound.clone());
        for (e, c) in &s.fire_effects {
            effects.extend(e.clone());
            sounds.extend(c.clone());
        }
        if let Some(n) = &s.next {
            stage_assets(n, effects, sounds);
        }
    }
    for spell in book.spells.values() {
        effects.extend(spell.cast_effect.clone());
        stage_assets(&spell.stage, &mut effects, &mut sounds);
    }
    for stage in book.shots.iter().flatten() {
        stage_assets(stage, &mut effects, &mut sounds);
    }
    effects.extend(book.hit_effect.clone());
    // What the core names (combat, status, the shotlock's own).
    effects.extend(
        book.by_name
            .values()
            .filter(|p| p.to_ascii_lowercase().starts_with("/game/effects/"))
            .cloned(),
    );
    // And the voice groups' lines.
    sounds.extend(
        book.by_name
            .values()
            .filter(|p| p.to_ascii_lowercase().starts_with("/game/sound/voice/"))
            .cloned(),
    );
    effects.sort_unstable();
    effects.dedup();
    // Effects, with what they spawn.
    let mut defs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut materials: Vec<String> = Vec::new();
    let mut meshes: Vec<String> = Vec::new();
    let mut todo = effects;
    let mut skipped = 0;
    while let Some(path) = todo.pop() {
        if !seen.insert(path.to_ascii_lowercase()) {
            continue;
        }
        let Ok(built) = effect_json::build(paks, &path) else {
            skipped += 1;
            continue;
        };
        match EffectDef::from_json(&built.json) {
            Ok(def) => defs.push(def),
            Err(_) => skipped += 1,
        }
        materials.extend(built.materials);
        meshes.extend(built.meshes);
        todo.extend(built.nested);
    }
    materials.sort_unstable();
    materials.dedup();
    meshes.sort_unstable();
    meshes.dedup();
    let mut mesh_data = Vec::new();
    for path in &meshes {
        if let Ok(mesh) = StaticMesh::load(paks, path) {
            mesh_data.push((effect_json::base_name(path), mesh));
        }
    }
    let mut fx_materials = Vec::new();
    let mut texture_paths: Vec<String> = Vec::new();
    let mut reader = kh3_shaded::Reader::new(1. / kh3_player::tuning::length_of(1.));
    let mut shaded = HashMap::new();
    for path in &materials {
        if let Ok(m) = material::read(paks, path) {
            let fx = fx_material(&m);
            texture_paths.extend(fx.texture.clone());
            texture_paths.extend(fx.opacity.clone());
            if let Some(record) = reader.record(paks, path, &m) {
                shaded.insert(effect_json::base_name(path), record);
            }
            fx_materials.push((effect_json::base_name(path), fx));
        }
    }
    // Meshes' own slot materials.
    for (_, mesh) in &mesh_data {
        for path in &mesh.materials {
            if path.is_empty() || materials.contains(path) {
                continue;
            }
            if let Ok(m) = material::read(paks, path) {
                let fx = fx_material(&m);
                texture_paths.extend(fx.texture.clone());
                texture_paths.extend(fx.opacity.clone());
                if let Some(record) = reader.record(paks, path, &m) {
                    shaded.insert(effect_json::base_name(path), record);
                }
                fx_materials.push((effect_json::base_name(path), fx));
            }
        }
    }
    if let Ok(dir) = std::env::var("KHC_SHADER_DUMP") {
        // [Test] The translated shaders and what each material binds, to read them.
        for (i, v) in reader.variants.iter().enumerate() {
            let _ = std::fs::write(format!("{dir}/v{i}.wgsl"), &v.source);
        }
        let mut list = String::new();
        for (name, r) in &shaded {
            list.push_str(&format!(
                "{name} sprite {:?} mesh {:?} vs {:?} decal {:?} adds {} blend {:?} textures {:?} cb {:?}\n",
                r.sprite, r.mesh, r.mesh_vertex, r.decal, r.decal_adds, r.blend, r.textures, r.cb
            ));
        }
        let _ = std::fs::write(format!("{dir}/materials.txt"), list);
    }
    let mut refused: Vec<_> = reader.refused.iter().collect();
    refused.sort_by_key(|r| std::cmp::Reverse(*r.1));
    info!(
        "KH3 shaders: {} materials translated, {} shaders; not translated: {:?}",
        shaded.len(),
        reader.variants.len(),
        refused.iter().take(12).collect::<Vec<_>>()
    );
    for record in shaded.values() {
        let used = record
            .variants()
            .flat_map(|v: usize| reader.variants[v].textures.iter().copied());
        for slot in used {
            if let Some(Some(path)) = record.textures.get(slot as usize) {
                texture_paths.push(path.clone());
            }
        }
    }
    texture_paths.sort_unstable();
    texture_paths.dedup();
    let mut textures = HashMap::new();
    for path in &texture_paths {
        let Ok(Some(p)) = paks.game_package(path) else {
            continue;
        };
        if let Some(i) = p.export_of_class("Texture2D")
            && let Ok(t) = Texture::read(&p, i)
        {
            textures.insert(path.clone(), t);
        }
    }
    for (_, m) in &mut fx_materials {
        m.mono = m
            .texture
            .as_ref()
            .and_then(|t| textures.get(t))
            .is_some_and(|t| matches!(t.format.as_str(), "PF_BC4" | "PF_G8"));
    }
    // Sounds through the effects' SeAttach data.
    let mut effect_sounds: HashMap<String, Vec<String>> = HashMap::new();
    for name in defs.iter().map(|d| d.name.clone()) {
        for folder in ["pc", "magic", "system", "common", "weapon"] {
            let path = format!("/Game/Sound/SE/_Data/SeAttach/{folder}/{name}");
            let Ok(Some(p)) = paks.game_package(&path) else {
                continue;
            };
            let Some(i) = p.export_of_class("SQEX_EffectSeAttachDataAsset") else {
                continue;
            };
            let Ok((props, _)) = kh3_data::props::export_props(&p, i) else {
                continue;
            };
            let mut list = Vec::new();
            for row in props.array("SEDatas").unwrap_or(&[]) {
                if let Some(s) = row
                    .as_props()
                    .and_then(|r| r.object("Sound"))
                    .and_then(|o| p.import_package(o))
                {
                    list.push(s.to_owned());
                }
            }
            if !list.is_empty() {
                sounds.extend(list.iter().cloned());
                effect_sounds.insert(name.clone(), list);
            }
            break;
        }
    }
    sounds.sort_unstable();
    sounds.dedup();
    let book = load_sounds(paks, install, &sounds)?;
    info!(
        "KH3 effects: {} systems ({skipped} skipped), {} materials, {} meshes, {} textures, {} of {} sounds",
        defs.len(),
        fx_materials.len(),
        mesh_data.len(),
        textures.len(),
        book.sounds.len(),
        sounds.len(),
    );
    Ok(FxData {
        defs,
        shaded,
        variants: reader.variants,
        materials: fx_materials,
        meshes: mesh_data,
        triggers,
        textures,
        sounds: book,
        effect_sounds,
        feet,
        ead: AttachRows(ead),
    })
}

fn load_sounds(
    paks: &Paks,
    install: &kh3_data::Install,
    sounds: &[String],
) -> Result<SoundBook, String> {
    let image = std::fs::read(install.exe()).map_err(|e| e.to_string())?;
    let tables = kh3_data::hca::Tables::find(&image).map_err(|e| e.to_string())?;
    let banks = Banks::default();
    // The XOR table, from the first obfuscated bank.
    let bank = banks
        .resolve(paks, &format!("{SWING_BANK}/se02001_101"))
        .ok_or("the Keyblade's sound bank is missing")?;
    let key = sead::find_key(&image, &tables, &bank.bank).map_err(|e| e.to_string())?;
    drop(image);
    let keys = Keys {
        tables,
        sead_key: key,
    };
    let mut clips: HashMap<(u16, u16), Option<audio::Clip>> = HashMap::new();
    let mut book = SoundBook::default();
    for path in sounds {
        let Some(r) = banks.resolve(paks, path) else {
            continue;
        };
        let Some(Some(sound)) = r.bank.sounds.get(r.slot) else {
            continue;
        };
        let mut sequences = Vec::new();
        for seq in &sound.sequences {
            let mut notes = Vec::new();
            for n in &seq.notes {
                let clip = clips
                    .entry((r.bank.number, n.material))
                    .or_insert_with(|| {
                        let m = r.bank.materials.get(usize::from(n.material))?;
                        let d = sead::decode_material(m, &keys).ok()?;
                        Some(audio::Clip {
                            channels: u16::from(d.channels),
                            rate: d.rate,
                            samples: d.samples.into(),
                            loop_frames: d.looped,
                        })
                    })
                    .clone();
                if let Some(clip) = clip {
                    notes.push(PlayNote {
                        delay: n.delay_ms as f32 / 1000.,
                        clip,
                        volume: n.volume,
                        volume_random: n.volume_random,
                        pitch_random: n.pitch_random,
                    });
                }
            }
            sequences.push((seq.weight, seq.group, notes));
        }
        // Macros 1, 3, 5, 7: the same sound started within the window is cancelled.
        let window = match sound.start_macro {
            1 => 0.067,
            3 => 0.133,
            5 => 0.266,
            7 => 0.533,
            10 => 2.,
            _ => 0.,
        };
        book.sounds.insert(
            path.clone(),
            Arc::new(Playable {
                kind: sound.kind,
                volume: sound.volume,
                asset_volume: r.volume,
                spatial: r.spatial,
                window,
                sequences,
                cursor: AtomicUsize::new(0),
            }),
        );
    }
    Ok(book)
}

/// A live effect: which Sora and socket it follows.
struct Live {
    id: EffectId,
    seat: u8,
    row: Option<EffectRow>,
    /// The clip's play it belongs to, for its end.
    serial: u32,
    trail: bool,
    state_end: Option<f32>,
    /// The core's handle of an effect it spawned on Sora.
    handle: Option<u32>,
}

/// One Sora's replay state.
#[derive(Default)]
struct Replay {
    serial: u32,
    clip: String,
    time: f32,
    /// Held sounds' mixer keys, when they end and the sound that follows.
    held: Vec<(u32, f32, Option<String>)>,
    /// The auto SE's enable windows' ends (their value lasts until then).
    auto_enable: Option<(bool, f32)>,
}

/// A sound scheduled in the near future.
struct Pending {
    at: f32,
    clip: audio::Clip,
    gain: f32,
    pitch: f32,
    key: u32,
}

#[derive(Resource, Default)]
pub struct Kh3Fx {
    /// The screen passes this frame's effects ask for.
    screen: Vec<kh3_cascade::screen::ScreenFx>,
    /// Translated materials drew this frame.
    drawing: bool,
    world: Option<World>,
    data: Option<Arc<FxData>>,
    materials: HashMap<u32, (Option<Handle<Image>>, MatBlend, [f32; 3], bool)>,
    meshes: HashMap<u32, StaticMesh>,
    /// A mesh's own first material, for an emitter that names none.
    mesh_materials: HashMap<u32, u32>,
    live: Vec<Live>,
    /// The spell actors' body effects: seat, the actor's serial, the effect.
    bodies: Vec<(u8, u32, EffectId)>,
    replays: [Replay; 2],
    accumulator: f32,
    batches: HashMap<(u32, u8), Batch>,
    effect_materials: HashMap<(u32, u8), Handle<EffectMaterial>>,
    pending: Vec<Pending>,
    /// Effects of a request with the time they stop looping.
    timed: Vec<(EffectId, f32)>,
    rim_materials: HashSet<u32>,
    /// The floor's height in KH3's space, cm: what soft particles fade against.
    pub floor: f32,
    shaded: HashMap<u32, kh3_shaded::Record>,
    shaded_images: HashMap<String, Handle<Image>>,
    shared_samplers: Option<(Handle<Image>, Handle<Image>)>,
    variant_textures: Vec<std::collections::BTreeSet<u32>>,
    shaded_batches: HashMap<(u32, u8, u8), ShadedBatch>,
    packed_materials: HashSet<u32>,
    mesh_normals: HashMap<u32, Vec<glam::Vec3>>,
    last_start: HashMap<String, f32>,
    clock: f32,
    next_key: u32,
    /// The sound being started may loop until its key stops.
    holding: bool,
    rng: u32,
    draw: DrawList,
    /// Each Sora's auto SE detector, and when it last ran.
    autose: [kh3_autose::AutoSe; 2],
    auto_clock: [f32; 2],
    /// The arena's surface in `FsTable_Sora`.
    pub surface: usize,
}

struct Batch {
    entity: Entity,
    mesh: Handle<Mesh>,
}

struct ShadedBatch {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<kh3_shaded::ShadedMaterial>,
}

#[derive(Component)]
pub struct Kh3FxPart;

/// A mono or masked texture as RGBA, whose alpha the shader can use.
fn bc4_block(b: &[u8], out: &mut [u8; 16]) {
    let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
    let mut table = [0u32; 8];
    table[0] = a0;
    table[1] = a1;
    if a0 > a1 {
        for i in 1..7 {
            table[i + 1] = ((7 - i as u32) * a0 + i as u32 * a1) / 7;
        }
    } else {
        for i in 1..5 {
            table[i + 1] = ((5 - i as u32) * a0 + i as u32 * a1) / 5;
        }
        table[6] = 0;
        table[7] = 255;
    }
    let bits = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], 0, 0]);
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = table[((bits >> (3 * i)) & 7) as usize] as u8;
    }
}

/// Mip level with the longest side at most `limit`, decoded to one channel
/// when the format allows it (BC4, G8, the alpha of DXT5).
fn single_channel(t: &Texture, limit: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mip = t
        .mips
        .iter()
        .find(|m| m.width.max(m.height) <= limit)
        .or(t.mips.last())?;
    let (w, h) = (mip.width as usize, mip.height as usize);
    match t.format.as_str() {
        "PF_G8" => Some((mip.width, mip.height, mip.bytes.clone())),
        "PF_BC4" | "PF_DXT5" => {
            let stride = if t.format == "PF_BC4" { 8 } else { 16 };
            let mut out = vec![0u8; w * h];
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            for by in 0..bh {
                for bx in 0..bw {
                    let block = mip.bytes.get((by * bw + bx) * stride..)?;
                    let mut px = [0u8; 16];
                    bc4_block(block, &mut px);
                    for (i, v) in px.iter().enumerate() {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x < w && y < h {
                            out[y * w + x] = *v;
                        }
                    }
                }
            }
            Some((mip.width, mip.height, out))
        }
        _ => None,
    }
}

/// A colour texture's mip at most `limit` wide as RGBA8 (BC1, BC3, BGRA; BC7
/// has no decoder here: `None`).
fn rgba_of(t: &Texture, limit: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mip = t
        .mips
        .iter()
        .find(|m| m.width.max(m.height) <= limit)
        .or(t.mips.last())?;
    let (w, h) = (mip.width as usize, mip.height as usize);
    let mut out = vec![255u8; w * h * 4];
    match t.format.as_str() {
        "PF_B8G8R8A8" => {
            for (o, c) in out.chunks_exact_mut(4).zip(mip.bytes.chunks_exact(4)) {
                o.copy_from_slice(&[c[2], c[1], c[0], c[3]]);
            }
        }
        "PF_DXT1" | "PF_DXT5" => {
            let stride = if t.format == "PF_DXT1" { 8 } else { 16 };
            let at = if t.format == "PF_DXT1" { 0 } else { 8 };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            for by in 0..bh {
                for bx in 0..bw {
                    let block = mip.bytes.get((by * bw + bx) * stride..)?;
                    let c0 = u16::from_le_bytes([block[at], block[at + 1]]);
                    let c1 = u16::from_le_bytes([block[at + 2], block[at + 3]]);
                    let rgb = |c: u16| {
                        [
                            u32::from(c >> 11) * 255 / 31,
                            u32::from((c >> 5) & 63) * 255 / 63,
                            u32::from(c & 31) * 255 / 31,
                        ]
                    };
                    let (a, b) = (rgb(c0), rgb(c1));
                    let mix = |x: u32, y: u32, n: u32, d: u32| (x * (d - n) + y * n) / d;
                    let table: [[u32; 4]; 4] = if c0 > c1 || at == 8 {
                        [
                            [a[0], a[1], a[2], 255],
                            [b[0], b[1], b[2], 255],
                            [
                                mix(a[0], b[0], 1, 3),
                                mix(a[1], b[1], 1, 3),
                                mix(a[2], b[2], 1, 3),
                                255,
                            ],
                            [
                                mix(a[0], b[0], 2, 3),
                                mix(a[1], b[1], 2, 3),
                                mix(a[2], b[2], 2, 3),
                                255,
                            ],
                        ]
                    } else {
                        [
                            [a[0], a[1], a[2], 255],
                            [b[0], b[1], b[2], 255],
                            [
                                mix(a[0], b[0], 1, 2),
                                mix(a[1], b[1], 1, 2),
                                mix(a[2], b[2], 1, 2),
                                255,
                            ],
                            [0, 0, 0, 0],
                        ]
                    };
                    let bits = u32::from_le_bytes([
                        block[at + 4],
                        block[at + 5],
                        block[at + 6],
                        block[at + 7],
                    ]);
                    let mut alpha = [255u8; 16];
                    if at == 8 {
                        bc4_block(block, &mut alpha);
                    }
                    for i in 0..16 {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x < w && y < h {
                            let c = table[((bits >> (2 * i)) & 3) as usize];
                            let o = (y * w + x) * 4;
                            out[o] = c[0] as u8;
                            out[o + 1] = c[1] as u8;
                            out[o + 2] = c[2] as u8;
                            out[o + 3] = if at == 8 { alpha[i] } else { c[3] as u8 };
                        }
                    }
                }
            }
        }
        _ => return None,
    }
    Some((mip.width, mip.height, out))
}

/// One channel of a mask texture, the alpha when it has one that varies.
fn mask_of(t: &Texture, limit: u32) -> Option<(u32, u32, Vec<u8>)> {
    if matches!(t.format.as_str(), "PF_BC4" | "PF_G8") {
        return single_channel(t, limit);
    }
    let (w, h, px) = rgba_of(t, limit)?;
    let use_alpha = px.chunks_exact(4).any(|c| c[3] < 250);
    let ch = if use_alpha { 3 } else { 0 };
    Some((w, h, px.chunks_exact(4).map(|c| c[ch]).collect()))
}

/// A colour under a mask: the reference's emissive with its separate opacity.
fn masked_image(color: Option<&Texture>, mask: &Texture) -> Option<Image> {
    let (w, h, a) = mask_of(mask, 512)?;
    let tiny = color.is_none_or(|t| t.width <= 8 && t.height <= 8);
    let colour = if tiny {
        None
    } else {
        color.and_then(|t| {
            if matches!(t.format.as_str(), "PF_BC4" | "PF_G8") {
                single_channel(t, 512).map(|(cw, ch, v)| {
                    let px: Vec<u8> = v.iter().flat_map(|x| [*x, *x, *x, 255]).collect();
                    (cw, ch, px)
                })
            } else {
                rgba_of(t, 512)
            }
        })
    };
    let mut px = Vec::with_capacity(a.len() * 4);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let alpha = a[y * w as usize + x];
            let c = colour.as_ref().map_or([255, 255, 255], |(cw, ch, p)| {
                let (cx, cy) = (x * *cw as usize / w as usize, y * *ch as usize / h as usize);
                let o = (cy * *cw as usize + cx) * 4;
                [p[o], p[o + 1], p[o + 2]]
            });
            px.extend_from_slice(&[c[0], c[1], c[2], alpha]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler_of(mask);
    Some(image)
}

/// `AddressX` / `AddressY` of a texture (the class default wraps).
fn address(t: &Texture, name: &str) -> ImageAddressMode {
    match t.props.str(name) {
        Some(mode) if mode.ends_with("TA_Clamp") => ImageAddressMode::ClampToEdge,
        Some(mode) if mode.ends_with("TA_Mirror") => ImageAddressMode::MirrorRepeat,
        _ => ImageAddressMode::Repeat,
    }
}

fn sampler_of(t: &Texture) -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address(t, "AddressX"),
        address_mode_v: address(t, "AddressY"),
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    })
}

fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    })
}

/// A texture the GPU cannot take as stored (a thin gradient has no 4 x 4 mip), decoded.
fn decoded_image(t: &Texture) -> Option<Image> {
    let (w, h, px) = rgba_of(t, 512).or_else(|| {
        single_channel(t, 512).map(|(w, h, v)| {
            let px = v.iter().flat_map(|x| [*x, *x, *x, 255]).collect();
            (w, h, px)
        })
    })?;
    let mut image = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler_of(t);
    Some(image)
}

/// A texture as stored: UNORM, because effects blend on stored values.
fn stored_image(t: &Texture) -> Option<Image> {
    let format = match t.format.as_str() {
        "PF_DXT1" => TextureFormat::Bc1RgbaUnorm,
        "PF_DXT5" => TextureFormat::Bc3RgbaUnorm,
        "PF_BC7" => TextureFormat::Bc7RgbaUnorm,
        "PF_B8G8R8A8" => TextureFormat::Bgra8Unorm,
        _ => return None,
    };
    let usable: Vec<_> = t
        .mips
        .iter()
        .take_while(|m| m.width >= 4 && m.height >= 4)
        .collect();
    let first = usable.first()?;
    let mut image = Image::default();
    image.texture_descriptor.size = Extent3d {
        width: first.width,
        height: first.height,
        depth_or_array_layers: 1,
    };
    image.texture_descriptor.dimension = TextureDimension::D2;
    image.texture_descriptor.format = format;
    image.texture_descriptor.mip_level_count = usable.len() as u32;
    image.data = Some(
        usable
            .iter()
            .flat_map(|m| m.bytes.iter().copied())
            .collect(),
    );
    image.sampler = sampler_of(t);
    Some(image)
}

/// Mono values to a white texture with that alpha (their "x^2 x tint").
fn mono_image(w: u32, h: u32, v: &[u8], opaque: bool) -> Image {
    let mut px = Vec::with_capacity(v.len() * 4);
    for x in v {
        let c = if opaque { *x } else { 255 };
        px.extend_from_slice(&[c, c, c, if opaque { 255 } else { *x }]);
    }
    let mut image = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler();
    image
}

/// A mesh's vertex normals from its faces, for the rim materials.
fn smooth_normals(mesh: &StaticMesh) -> Vec<glam::Vec3> {
    let mut normals = vec![glam::Vec3::ZERO; mesh.positions.len()];
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        let (Some(pa), Some(pb), Some(pc)) = (
            mesh.positions.get(a),
            mesh.positions.get(b),
            mesh.positions.get(c),
        ) else {
            continue;
        };
        let [pa, pb, pc] = [pa, pb, pc].map(|p| glam::Vec3::from_array(*p));
        let face = (pb - pa).cross(pc - pa);
        for i in [a, b, c] {
            normals[i] += face;
        }
    }
    normals.iter().map(|n| n.normalize_or_zero()).collect()
}

/// How much of a mesh with a masks-only material shows.
const PACKED_MESH_ALPHA: f32 = 0.35;

/// How much a vertex shows by the rim: nothing face on, all of it edge on.
fn rim(normal: glam::Vec3, to_eye: glam::Vec3) -> f32 {
    (1. - normal.dot(to_eye).abs()).clamp(0., 1.).powi(3)
}

impl Kh3Fx {
    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    pub fn screen_passes(&self) -> &[kh3_cascade::screen::ScreenFx] {
        &self.screen
    }

    /// Takes over what the load thread read.
    fn start(&mut self, data: Arc<FxData>, images: &mut Assets<Image>) {
        let mut world = World::new();
        for def in &data.defs {
            world.register(def.clone());
        }
        // [Test] What the effects were read as.
        if let Ok(dir) = std::env::var("KHC_SHADER_DUMP") {
            let mut t = String::new();
            for def in &data.defs {
                t.push_str(&format!(
                    "=== {} base_scale {:?}\n",
                    def.name, def.base_scale
                ));
                for e in &def.emitters {
                    t.push_str(&format!("-T {} auto {} transforms {:?} init {:?} inherit {:?} children {:?} shape {:?}\n", e.name, e.auto_add, e.transforms, e.init, e.inherit, e.children, e.shape()));
                    t.push_str(&format!(
                        "-- {} mesh {:?} modules {:?}\n",
                        e.name, e.mesh, e.modules
                    ));
                    for s in &e.screen {
                        t.push_str(&format!("-S {} {:?} {s:?}\n", e.name, s.kind));
                    }
                    if !e.char_params.is_empty() {
                        t.push_str(&format!("-C {} {:?}\n", e.name, e.char_params));
                    }
                }
            }
            let _ = std::fs::write(format!("{dir}/systems.txt"), t);
        }
        let mut textures: HashMap<String, Option<Handle<Image>>> = HashMap::new();
        for (name, m) in &data.materials {
            let Some(path) = m.texture.as_ref().or(m.opacity.as_ref()) else {
                // No texture at all: a mesh's plain colour.
                if m.flat {
                    let id = world.intern_material(name);
                    self.materials.insert(id, (None, m.blend, m.tint, m.mono));
                    if m.rim {
                        self.rim_materials.insert(id);
                    }
                }
                continue;
            };
            let additive = m.blend == MatBlend::Additive;
            let handle = textures
                .entry(format!("{path}|{:?}|{additive}|{}", m.opacity, m.packed))
                .or_insert_with(|| {
                    if let Some(mask) = m.opacity.as_ref().and_then(|o| data.textures.get(o)) {
                        // A packed texture's channels are masks, no colour to put under one.
                        let color = m
                            .texture
                            .as_ref()
                            .filter(|_| !m.packed)
                            .and_then(|c| data.textures.get(c));
                        if let Some(image) = masked_image(color, mask) {
                            return Some(images.add(image));
                        }
                    }
                    let t = data.textures.get(path)?;
                    if m.packed
                        && let Some((w, h, v)) = gray_of(t, 512)
                    {
                        return Some(images.add(mono_image(w, h, &v, additive)));
                    }
                    let image = match single_channel(t, 512) {
                        Some((w, h, v)) if matches!(t.format.as_str(), "PF_BC4" | "PF_G8") => {
                            mono_image(w, h, &v, additive)
                        }
                        _ => match stored_image(t).or_else(|| decoded_image(t)) {
                            Some(image) => image,
                            None => {
                                warn!("KH3 effects: no image from {path} ({})", t.format);
                                return None;
                            }
                        },
                    };
                    Some(images.add(image))
                })
                .clone();
            // A texture that cannot be shown: no plain shape in its place.
            if handle.is_none() {
                continue;
            }
            let id = world.intern_material(name);
            self.materials.insert(id, (handle, m.blend, m.tint, m.mono));
            if m.rim {
                self.rim_materials.insert(id);
            }
            if m.packed {
                self.packed_materials.insert(id);
            }
        }
        // The translated materials and what their shaders read.
        let shared = kh3_shaded::shared_samplers(images);
        for (name, record) in &data.shaded {
            let used = record
                .variants()
                .flat_map(|v| data.variants[v].textures.iter().copied());
            for slot in used {
                if let Some(Some(path)) = record.textures.get(slot as usize)
                    && !self.shaded_images.contains_key(path)
                    && let Some(image) = data.textures.get(path).and_then(kh3_shaded::image_of)
                {
                    self.shaded_images.insert(path.clone(), images.add(image));
                }
            }
            self.shaded
                .insert(world.intern_material(name), record.clone());
        }
        self.shared_samplers = Some(shared);
        self.variant_textures = data.variants.iter().map(|v| v.textures.clone()).collect();
        for (name, mesh) in &data.meshes {
            let id = world.intern_mesh(name);
            self.mesh_normals.insert(id, smooth_normals(mesh));
            self.meshes.insert(id, mesh.clone());
            if let Some(first) = mesh.materials.first().filter(|m| !m.is_empty()) {
                let material = world.intern_material(&effect_json::base_name(first));
                self.mesh_materials.insert(id, material);
            }
        }
        self.world = Some(world);
        self.data = Some(data);
        self.rng = 0x1234_5678;
        self.next_key = 1;
    }

    fn random(&mut self) -> f32 {
        let mut x = self.rng.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// `(x, y, z)` in the room to KH3's space (cm, Z up, left-handed).
pub fn to_kh3(p: Vec3) -> glam::Vec3 {
    let k = 1. / kh3_player::tuning::length_of(1.);
    glam::Vec3::new(p.x * k, p.z * k, p.y * k)
}

pub fn from_kh3(p: glam::Vec3) -> [f32; 3] {
    let k = kh3_player::tuning::length_of(1.);
    [p.x * k, p.z * k, p.y * k]
}

fn axis(v: Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.z, v.y)
}

/// The Sora's pose for sockets: his place and turn in KH3's space, and his
/// bones in the mesh's.
pub struct Body<'a> {
    pub seat: u8,
    pub at: glam::Vec3,
    pub yaw: f32,
    pub component: &'a [BonePose],
}

fn quat_mat(q: [f32; 4]) -> glam::Mat3 {
    glam::Mat3::from_quat(glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]))
}

/// A socket's transform in KH3's space.
fn socket_xform(data: &SoraData, body: &Body, name: &str, on_weapon: bool) -> Option<Xform> {
    let bone_pose = |bone: &str| data.bone(bone).and_then(|b| body.component.get(b)).copied();
    let local = |s: &sora::Socket| BonePose {
        rotation: sora::quat_of_rotator(s.rotation),
        translation: s.location,
        scale: s.scale,
    };
    let pose = if on_weapon {
        let grip = data.sockets.iter().find(|s| s.name == "R_wep_skt")?;
        let hand = bone_pose(&grip.bone)?;
        let root = sora::compose(&hand, &local(grip));
        let sock = data.keyblade_sockets.iter().find(|s| s.name == name);
        match sock {
            Some(s) => {
                let bones = &data.keyblade.bones;
                let at = bones.iter().position(|b| b.name == s.bone)?;
                let mut chain = BonePose {
                    rotation: bones[at].rotation,
                    translation: bones[at].translation,
                    scale: bones[at].scale,
                };
                let mut up = bones[at].parent;
                while let Ok(i) = usize::try_from(up) {
                    let b = bones.get(i)?;
                    let p = BonePose {
                        rotation: b.rotation,
                        translation: b.translation,
                        scale: b.scale,
                    };
                    chain = sora::compose(&p, &chain);
                    up = b.parent;
                }
                sora::compose(&sora::compose(&root, &chain), &local(s))
            }
            None => root,
        }
    } else if let Some(s) = data.sockets.iter().find(|s| s.name == name) {
        sora::compose(&bone_pose(&s.bone)?, &local(s))
    } else {
        bone_pose(name).unwrap_or(BonePose {
            rotation: [0., 0., 0., 1.],
            translation: [0.; 3],
            scale: [1.; 3],
        })
    };
    // His turn: the room's yaw about Y is a turn about KH3's Z the other way
    // (the swap of Y and Z flips the hand).
    let actor = glam::Mat3::from_rotation_z(-body.yaw);
    let t = glam::Vec3::from_array(pose.translation);
    Some(Xform {
        position: body.at + actor * t,
        rotation: actor * quat_mat(pose.rotation),
        scale: glam::Vec3::from_array(pose.scale),
    })
}

fn row_xform(base: &Xform, row: &EffectRow) -> Xform {
    let offset = glam::Vec3::from_array(row.location);
    let r = kh3_cascade::rotator(row.rotation[0], row.rotation[1], row.rotation[2]);
    Xform {
        position: base.position + base.rotation * offset,
        rotation: base.rotation * r,
        scale: base.scale * glam::Vec3::from_array(row.scale),
    }
}

/// Starts a sound's notes.
fn play_sound(
    fx: &mut Kh3Fx,
    path: &str,
    volume: f32,
    pitch: f32,
    at: Option<glam::Vec3>,
    camera: glam::Vec3,
    key: Option<u32>,
) {
    let Some(data) = fx.data.clone() else { return };
    let Some(sound) = data.sounds.sounds.get(path) else {
        return;
    };
    let now = fx.clock;
    if sound.window > 0.
        && fx
            .last_start
            .get(path)
            .is_some_and(|t| now - *t < sound.window)
    {
        return;
    }
    fx.last_start.insert(path.to_owned(), now);
    let mut distance_gain = 1.;
    if sound.spatial
        && let Some(p) = at
    {
        let d = (p - camera).length();
        distance_gain = if d <= 400. {
            1.
        } else {
            ((4000. - d) / 3600.).max(0.)
        };
    }
    // The sequences to play: all for a Sound, one weighted group else.
    let picked: Vec<usize> = match sound.kind {
        SoundKind::Sound | SoundKind::Empty | SoundKind::Switch => {
            (0..sound.sequences.len()).collect()
        }
        SoundKind::Order => {
            let n = sound.sequences.len().max(1);
            let i = sound.cursor.fetch_add(1, Ordering::Relaxed) % n;
            vec![i]
        }
        SoundKind::Random | SoundKind::Cycle => {
            let r = fx.random() * 101.;
            let mut sum = 0.;
            let mut pick = 0;
            for (i, s) in sound.sequences.iter().enumerate() {
                sum += f32::from(s.0);
                if r <= sum {
                    pick = i;
                    break;
                }
            }
            vec![pick]
        }
    };
    let base = sound.volume * sound.asset_volume * volume * distance_gain;
    for i in picked {
        let Some((_, _, notes)) = sound.sequences.get(i) else {
            continue;
        };
        for n in notes {
            let (mut gain, mut rate) = (base * n.volume, pitch);
            if let Some((max, min, mode)) = n.volume_random {
                let v = min + fx.random() * (max - min);
                gain = if mode == 2 { gain * v } else { gain + v };
            }
            if let Some((max, min, mode)) = n.pitch_random {
                let v = min + fx.random() * (max - min);
                rate = if mode == 2 { rate * v } else { rate + v };
            }
            let key = key.unwrap_or_else(|| {
                fx.next_key = fx.next_key.wrapping_add(1) & 0xff;
                KEY_BASE | 0x8000 | fx.next_key
            });
            let mut clip = n.clip.clone();
            if !fx.holding {
                clip.loop_frames = None;
            }
            fx.pending.push(Pending {
                at: now + n.delay,
                clip,
                gain: gain.clamp(0., 1.),
                pitch: rate.clamp(1. / 512., 2.),
                key,
            });
        }
    }
}

/// Stops a mixer key on the next frame's sounds.
fn stop_key(fx: &mut Kh3Fx, key: u32) {
    fx.pending.push(Pending {
        at: fx.clock,
        clip: audio::Clip {
            channels: 1,
            rate: 48000,
            samples: Arc::from(Vec::new()),
            loop_frames: None,
        },
        gain: 0.,
        pitch: 1.,
        key,
    });
}

/// Fires a play's notifies between two times.
#[allow(clippy::too_many_arguments)]
fn replay(
    fx: &mut Kh3Fx,
    data: &SoraData,
    body: &Body,
    index: u16,
    serial: u32,
    time: f32,
    x: f32,
    camera: glam::Vec3,
) {
    let Some(clip) = data.anims.main_clip(index, x) else {
        return;
    };
    let seat = usize::from(body.seat);
    let fresh = fx.replays[seat].serial != serial || fx.replays[seat].clip != clip.name;
    if fresh {
        // The clip changed: effects that end with it loop out, held sounds fade.
        for l in &mut fx.live {
            if l.seat == body.seat && l.serial == fx.replays[seat].serial {
                let ends = l.trail || l.row.as_ref().is_some_and(|r| r.ends_with_clip || r.state);
                if ends && let Some(w) = fx.world.as_mut() {
                    w.stop(l.id, false);
                    l.row = None;
                }
            }
        }
        let r = &mut fx.replays[seat];
        r.serial = serial;
        r.clip = clip.name.clone();
        r.time = -1e-4;
        r.auto_enable = None;
        let held = std::mem::take(&mut r.held);
        let at = socket_xform(data, body, "kubi", false).map(|x| x.position);
        for (key, _, end) in held {
            stop_key(fx, key);
            if let Some(end) = end {
                play_sound(fx, &end, 1., 1., at, camera, None);
            }
        }
    }
    let from = fx.replays[seat].time;
    if time < from {
        // A loop wrapped.
        fx.replays[seat].time = -1e-4;
    }
    let from = fx.replays[seat].time;
    fx.replays[seat].time = time;
    let ended: Vec<(u32, Option<String>)> = fx.replays[seat]
        .held
        .iter()
        .filter(|h| h.1 <= time)
        .map(|h| (h.0, h.2.clone()))
        .collect();
    fx.replays[seat].held.retain(|h| h.1 > time);
    let at = socket_xform(data, body, "kubi", false).map(|x| x.position);
    for (key, end) in ended {
        stop_key(fx, key);
        if let Some(end) = end {
            play_sound(fx, &end, 1., 1., at, camera, None);
        }
    }
    if fx.replays[seat].auto_enable.is_some_and(|e| e.1 <= time) {
        fx.replays[seat].auto_enable = None;
    }
    let Some(triggers) = fx
        .data
        .as_ref()
        .and_then(|d| d.triggers.get(&clip.name))
        .cloned()
    else {
        return;
    };
    for t in &triggers {
        // State ends.
        if t.end > from && t.end <= time {
            for l in &mut fx.live {
                if l.seat == body.seat
                    && l.serial == serial
                    && l.state_end == Some(t.end)
                    && let Some(w) = fx.world.as_mut()
                {
                    w.stop(l.id, false);
                    l.state_end = None;
                    l.trail = false;
                }
            }
        }
        if t.start <= from || t.start > time {
            continue;
        }
        match &t.cue {
            Cue::Effect(row_data) => {
                let Some(base) = socket_xform(data, body, &row_data.socket, row_data.on_weapon)
                else {
                    continue;
                };
                let name = effect_json::base_name(&row_data.effect).to_ascii_lowercase();
                let options = SpawnOptions {
                    seed: None,
                    color: row_data.color,
                    start_delay: row_data.delay,
                    fade_in: row_data.fade_in,
                };
                let Some(w) = fx.world.as_mut() else { continue };
                let Some(id) = w.spawn(&name, row_xform(&base, row_data), options) else {
                    continue;
                };
                fx.live.push(Live {
                    id,
                    seat: body.seat,
                    row: row_data.attach.then(|| row_data.clone()),
                    serial,
                    trail: false,
                    state_end: row_data.state.then_some(t.end),
                    handle: None,
                });
                let sounds = fx
                    .data
                    .as_ref()
                    .and_then(|d| d.effect_sounds.get(&name))
                    .cloned();
                for s in sounds.unwrap_or_default() {
                    play_sound(fx, &s, 1., 1., Some(base.position), camera, None);
                }
            }
            Cue::Trail => {
                let name = effect_json::base_name(TRAIL).to_ascii_lowercase();
                let Some(top) = socket_xform(data, body, "vfx_top", true) else {
                    continue;
                };
                let Some(w) = fx.world.as_mut() else { continue };
                if let Some(id) = w.spawn(&name, top, SpawnOptions::default()) {
                    fx.live.push(Live {
                        id,
                        seat: body.seat,
                        row: None,
                        serial,
                        trail: true,
                        handle: None,
                        state_end: Some(t.end),
                    });
                }
            }
            Cue::Sound {
                path,
                volume,
                pitch,
            } => {
                let at = socket_xform(data, body, "kubi", false).map(|x| x.position);
                play_sound(fx, path, *volume, *pitch, at, camera, None);
            }
            Cue::Voice(list) => {
                let total: u32 = list.iter().map(|v| v.1).sum();
                let mut r = (fx.random() * total as f32) as u32;
                let mut pick = list.first().map(|v| v.0.clone());
                for (p, w) in list {
                    if r < *w {
                        pick = Some(p.clone());
                        break;
                    }
                    r -= *w;
                }
                // One line at a time per Sora.
                let key = KEY_BASE | 0x100 | u32::from(body.seat);
                if let Some(p) = pick {
                    let at = socket_xform(data, body, "atama", false).map(|x| x.position);
                    stop_key(fx, key);
                    play_sound(fx, &p, 1., 1., at, camera, Some(key));
                }
            }
            Cue::Held { path, end } => {
                let at = socket_xform(data, body, "kubi", false).map(|x| x.position);
                fx.next_key = fx.next_key.wrapping_add(1) & 0xff;
                let key = KEY_BASE | 0x4000 | fx.next_key;
                fx.holding = true;
                play_sound(fx, path, 1., 1., at, camera, Some(key));
                fx.holding = false;
                fx.replays[seat].held.push((key, t.end, end.clone()));
            }
            Cue::AutoSe { motion, state } => {
                let path = fx
                    .data
                    .as_ref()
                    .and_then(|d| d.feet.sound(fx.surface, *motion).cloned());
                let Some(path) = path else {
                    continue;
                };
                let at = socket_xform(data, body, "L_ashi1", false).map(|x| x.position);
                if *state {
                    fx.next_key = fx.next_key.wrapping_add(1) & 0xff;
                    let key = KEY_BASE | 0x4000 | fx.next_key;
                    fx.holding = true;
                    play_sound(fx, &path, 1., 1., at, camera, Some(key));
                    fx.holding = false;
                    fx.replays[seat].held.push((key, t.end, None));
                } else {
                    play_sound(fx, &path, 1., 1., at, camera, None);
                }
            }
            Cue::AutoEnable(on) => {
                fx.replays[seat].auto_enable = Some((*on, t.end));
            }
            Cue::Rows { .. } => {}
        }
    }
}

/// A spell's or a shot's effect, by its package, for the room's magic.
pub fn spawn_at(
    fx: &mut Kh3Fx,
    effect: &str,
    at: [f32; 3],
    yaw: f32,
    camera: Vec3,
) -> Option<EffectId> {
    let name = effect_json::base_name(effect).to_ascii_lowercase();
    let place = to_kh3(Vec3::from_array(at));
    let world = fx.world.as_mut()?;
    let xform = Xform {
        position: place,
        rotation: glam::Mat3::from_rotation_z(-yaw)
            * glam::Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2),
        scale: glam::Vec3::ONE,
    };
    let id = world.spawn(&name, xform, SpawnOptions::default());
    let sounds = fx
        .data
        .as_ref()
        .and_then(|d| d.effect_sounds.get(&name))
        .cloned();
    for s in sounds.unwrap_or_default() {
        play_sound(fx, &s, 1., 1., Some(place), to_kh3(camera), None);
    }
    id
}

/// The HUD's static sounds: index in `TresStaticSESet.m_Assets` to the number of `se00000_NNN`.
const UI_SOUNDS: [(u32, u32); 11] = [
    (0, 1),
    (3, 4),
    (4, 5),
    (6, 7),
    (7, 8),
    (8, 9),
    (9, 10),
    (18, 39),
    (19, 42),
    (20, 43),
    (25, 51),
];

fn ui_sound_path(number: u32) -> String {
    format!("/Game/Sound/SE/common/00000/se00000_{number:03}")
}

/// A sound the HUD's movies asked for: flat, at no place. A loop holds its own mixer key.
pub fn play_ui(fx: &mut Kh3Fx, sound: kh3_ui::UiSound) {
    use kh3_ui::UiSound;
    let index = match sound {
        UiSound::Play(i) | UiSound::LoopStart(i) | UiSound::LoopStop(i) => i,
    };
    let Some(&(_, number)) = UI_SOUNDS.iter().find(|s| s.0 == index) else {
        return;
    };
    let path = ui_sound_path(number);
    let key = KEY_BASE | 0x2000 | index;
    match sound {
        UiSound::Play(_) => play_sound(fx, &path, 1., 1., None, Vec3::ZERO, None),
        UiSound::LoopStart(_) => {
            fx.holding = true;
            play_sound(fx, &path, 1., 1., None, Vec3::ZERO, Some(key));
            fx.holding = false;
            // The HUD asked for a loop: a clip with no loop points of its own loops whole.
            for p in fx.pending.iter_mut().filter(|p| p.key == key) {
                let frames = p.clip.samples.len() as u32 / u32::from(p.clip.channels.max(1));
                if p.clip.loop_frames.is_none() && frames > 0 {
                    p.clip.loop_frames = Some((0, frames));
                }
            }
        }
        UiSound::LoopStop(_) => stop_key(fx, key),
    }
}

pub fn play_at(fx: &mut Kh3Fx, sound: &str, at: [f32; 3], camera: Vec3) {
    play_sound(
        fx,
        sound,
        1.,
        1.,
        Some(to_kh3(Vec3::from_array(at))),
        to_kh3(camera),
        None,
    );
}

/// An effect the core put on a Sora or stops: it follows its bone (`body_step`) until stopped or done. The rows of an
/// attach-data asset play by the group (0x1405B9370).
fn attached(
    fx: &mut Kh3Fx,
    r: &kh3_player::spell::FxRequest,
    a: &kh3_player::spell::FxAttach,
    camera: Vec3,
) {
    if let Some(immediate) = a.stop {
        let mut live = std::mem::take(&mut fx.live);
        for l in &mut live {
            if l.seat == a.seat
                && l.handle == Some(a.handle)
                && let Some(w) = fx.world.as_mut()
            {
                w.stop(l.id, immediate);
                l.handle = None;
                l.row = None;
            }
        }
        fx.live = live;
        return;
    }
    let Some(effect) = &r.effect else { return };
    let name = effect_json::base_name(effect).to_ascii_lowercase();
    let group = i64::from(a.group);
    let rows: Vec<EffectRow> = match fx.data.as_ref().and_then(|d| d.ead.0.get(&name)) {
        Some(rows) => rows
            .iter()
            .filter(|(g, _)| *g == group || *g == -1)
            .map(|(_, row)| row.clone())
            .collect(),
        None => vec![EffectRow {
            effect: effect.clone(),
            socket: a.bone.clone().unwrap_or_else(|| "Trans".to_owned()),
            on_weapon: false,
            attach: true,
            location: a.offset,
            rotation: [0.; 3],
            scale: [1.; 3],
            fade_in: 0.,
            delay: 0.,
            color: [1.; 4],
            ends_with_clip: false,
            state: false,
        }],
    };
    for row in rows {
        let Some(id) = spawn_at(fx, &row.effect, r.at, r.yaw, camera) else {
            continue;
        };
        fx.live.push(Live {
            id,
            seat: a.seat,
            row: Some(row),
            serial: 0,
            trail: false,
            state_end: None,
            handle: Some(a.handle),
        });
    }
}

/// How long an effect nothing else will stop may go on looping, in seconds. Fire's scorch and
/// Aero's pad loop; what ends them in the game is not traced, so here they end alike.
const LINGER: f32 = 4.;

/// The requests a Sora's layer made this tick.
pub fn requests(fx: &mut Kh3Fx, list: Vec<kh3_player::spell::FxRequest>, camera: Vec3) {
    for r in list {
        if let Some(a) = &r.attach {
            attached(fx, &r, a, camera);
            continue;
        }
        if let Some(e) = &r.effect
            && let Some(id) = spawn_at(fx, e, r.at, r.yaw, camera)
        {
            fx.timed.push((id, fx.clock + LINGER));
        }
        if let Some(s) = &r.sound {
            play_at(fx, s, r.at, camera);
        }
    }
}

/// The spell actors' body effects: one per live actor, following it.
pub fn actors_step(
    fx: &mut Kh3Fx,
    seat: u8,
    actors: &[kh3_player::spell::Actor],
    bolts: &[kh3_player::Bolt],
    book: &kh3_player::attack::Book,
) {
    let Some(world) = fx.world.as_mut() else {
        return;
    };
    // A shot is an actor with the shot's stage.
    let shots: Vec<kh3_player::spell::Actor> = bolts
        .iter()
        .filter_map(|b| {
            let stage = book.shots[usize::from(b.ragnarok)].clone()?;
            Some(kh3_player::spell::Actor {
                serial: b.serial | 0x8000,
                cmd: 0,
                speed: 0.,
                stage,
                pos: b.pos,
                dir: b.dir,
                age: 0.,
                turn: 0.,
                homing_on: false,
                finish: false,
                grand: false,
                hit: false,
                previous: b.pos,
                dead: false,
            })
        })
        .collect();
    let all: Vec<&kh3_player::spell::Actor> = actors.iter().chain(&shots).collect();
    let live = &mut fx.bodies;
    live.retain(|(s, serial, id)| {
        if *s != seat {
            return true;
        }
        let here = all.iter().any(|a| a.serial == *serial);
        if !here {
            world.stop(*id, false);
        }
        here
    });
    for a in all {
        let place = to_kh3(Vec3::from_array(a.pos));
        let forward = glam::Vec3::new(a.dir[0], a.dir[2], a.dir[1]).normalize_or(glam::Vec3::X);
        let yaw = forward.y.atan2(forward.x);
        let pitch = forward.z.clamp(-1., 1.).asin();
        let rotation = kh3_cascade::rotator(pitch.to_degrees(), yaw.to_degrees(), 0.);
        let xform = Xform {
            position: place,
            rotation,
            scale: glam::Vec3::ONE,
        };
        if let Some(entry) = live.iter().find(|e| e.0 == seat && e.1 == a.serial) {
            world.set_transform(entry.2, xform);
        } else if let Some(body) = &a.stage.body_effect {
            let name = effect_json::base_name(body).to_ascii_lowercase();
            if let Some(id) = world.spawn(&name, xform, SpawnOptions::default()) {
                live.push((seat, a.serial, id));
            }
        }
    }
}

/// One Sora's step of the effects, from the model's update.
#[allow(clippy::too_many_arguments)]
pub fn body_step(
    fx: &mut Kh3Fx,
    data: &SoraData,
    seat: u8,
    position: [f32; 3],
    yaw: f32,
    component: &[BonePose],
    visual: &kh3_player::visual::Visual,
    row: u16,
    time_in: f32,
    camera: Vec3,
    grounded: bool,
) {
    if fx.world.is_none() {
        return;
    }
    let body = Body {
        seat,
        at: to_kh3(Vec3::from_array(position)),
        yaw,
        component,
    };
    replay(
        fx,
        data,
        &body,
        row,
        visual.serial,
        time_in,
        visual.at[0],
        to_kh3(camera),
    );
    footsteps(fx, data, &body, grounded, camera);
    // Attached effects follow their sockets; trails take the blade's edge.
    let mut live = std::mem::take(&mut fx.live);
    for l in &live {
        if l.seat != seat {
            continue;
        }
        let Some(w) = fx.world.as_mut() else { break };
        if l.trail {
            if let (Some(top), Some(bottom)) = (
                socket_xform(data, &body, "vfx_top", true),
                socket_xform(data, &body, "vfx_bottom", true),
            ) {
                w.set_transform(l.id, top);
                w.set_trail(l.id, top.position, bottom.position);
            }
        } else if let Some(row) = &l.row
            && let Some(base) = socket_xform(data, &body, &row.socket, row.on_weapon)
        {
            w.set_transform(l.id, row_xform(&base, row));
        }
    }
    if let Some(w) = fx.world.as_ref() {
        live.retain(|l| w.is_alive(l.id));
    }
    fx.live = live;
}

/// `SQEXSEADAutoSeComponent`: the detector at 60 Hz on the pose's bones, its sounds at their parts' bones.
fn footsteps(fx: &mut Kh3Fx, data: &SoraData, body: &Body, grounded: bool, camera: Vec3) {
    let seat = usize::from(body.seat);
    if seat >= fx.autose.len() {
        return;
    }
    let dt = fx.clock - fx.auto_clock[seat];
    if dt < 1. / 60. - 1e-4 {
        return;
    }
    fx.auto_clock[seat] = fx.clock;
    let mut bones = [glam::Vec3::ZERO; 14];
    let actor = glam::Mat3::from_rotation_z(-body.yaw);
    for (at, name) in bones.iter_mut().zip(kh3_autose::BONES) {
        let Some(b) = data.bone(name).and_then(|i| body.component.get(i)) else {
            return;
        };
        *at = body.at + actor * glam::Vec3::from_array(b.translation);
    }
    let frame = kh3_autose::Frame::of(&bones, grounded);
    let listener = (body.at - to_kh3(camera)).length() * 0.01;
    let Some(feet) = fx.data.clone() else {
        return;
    };
    let enabled = fx.replays[seat].auto_enable.is_none_or(|e| e.0);
    let events = fx.autose[seat].step(dt, &frame, listener, enabled, &feet.feet.settings);
    for e in events {
        let Some(path) = feet.feet.sound(fx.surface, e.motion) else {
            continue;
        };
        let at = socket_xform(data, body, e.part.bone(), false).map(|x| x.position);
        play_sound(fx, path, e.volume, 1., at, to_kh3(camera), None);
    }
}

/// Steps the evaluator at 60 Hz, starts sounds due and draws.
#[allow(clippy::too_many_arguments)]
pub fn frame(
    fx: &mut Kh3Fx,
    dt: f32,
    camera: Option<(Vec3, Vec3, Vec3, Vec3)>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    effect_materials: &mut Assets<EffectMaterial>,
    shaded_materials: &mut Assets<kh3_shaded::ShadedMaterial>,
    visibilities: &mut Query<&mut Visibility>,
    mixer: Option<&mut audio::Mixer>,
) {
    let Some(mut world) = fx.world.take() else {
        return;
    };
    fx.clock += dt;
    fx.accumulator += dt;
    let clock = fx.clock;
    fx.timed.retain(|&(id, end)| {
        let over = end <= clock;
        if over {
            world.stop(id, false);
        }
        !over
    });
    let mut steps = 0;
    while fx.accumulator >= 1. / 60. && steps < 4 {
        world.step(1. / 60.);
        fx.accumulator -= 1. / 60.;
        steps += 1;
    }
    if steps == 4 {
        fx.accumulator = 0.;
    }
    if let Some(mixer) = mixer {
        let clock = fx.clock;
        let mut i = 0;
        while i < fx.pending.len() {
            if fx.pending[i].at <= clock {
                let p = fx.pending.swap_remove(i);
                if p.clip.samples.is_empty() {
                    mixer.stop(p.key);
                } else {
                    mixer.play(p.clip, p.key, p.gain, p.pitch);
                }
            } else {
                i += 1;
            }
        }
    }
    if let Some((position, forward, right, up)) = camera {
        let camera = Camera {
            position: to_kh3(position),
            forward: axis(forward),
            right: axis(right),
            up: axis(up),
        };
        let mut list = std::mem::take(&mut fx.draw);
        list.clear();
        world.draw(fx.accumulator * 60., &camera, &mut list);
        let mut passes = kh3_cascade::screen::pick(&list.screen);
        for pass in &mut passes {
            if let (true, Some(at)) = (pass.fade_distance > 0., pass.world) {
                pass.alpha *=
                    (1. - at.distance(camera.position) / pass.fade_distance).clamp(0., 1.);
            }
        }
        fx.screen = passes;
        let eye = camera.position;
        draw_list(
            fx,
            eye,
            &list,
            commands,
            meshes,
            effect_materials,
            shaded_materials,
            visibilities,
        );
        fx.draw = list;
    }
    fx.world = Some(world);
}

#[derive(Default)]
struct Verts {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Verts {
    fn push(&mut self, p: glam::Vec3, uv: [f32; 2], c: [f32; 4]) -> u32 {
        let i = self.positions.len() as u32;
        self.positions.push(from_kh3(p));
        self.uvs.push(uv);
        self.colors.push(c);
        i
    }
}

fn gamma(c: [f32; 4], tint: [f32; 3]) -> [f32; 4] {
    let g = |x: f32, t: f32| (x * t).max(0.).powf(1. / 2.2);
    [
        g(c[0], tint[0]),
        g(c[1], tint[1]),
        g(c[2], tint[2]),
        c[3].clamp(0., 1.),
    ]
}

/// The pass of a draw: 0 translucent, 1 additive.
fn pass(mode: Blend, material: MatBlend) -> u8 {
    match mode {
        Blend::Additive => 1,
        Blend::Translucent => 0,
        Blend::Material => u8::from(material == MatBlend::Additive),
    }
}

/// The kinds of draw of a translated material: `CLASS_*`.
const CLASS_SPRITE: u8 = 0;
const CLASS_MESH: u8 = 1;
const CLASS_MESH_VERTEX: u8 = 2;
const CLASS_DECAL: u8 = 3;
const CLASS_SPRITE_DISTORT: u8 = 4;
const CLASS_MESH_DISTORT: u8 = 5;

/// The class of a mesh draw, by the shaders the material has: a mesh falls back on the sprite's.
fn mesh_class(record: &kh3_shaded::Record) -> Option<u8> {
    if record.mesh.is_some() {
        Some(if record.mesh_vertex.is_some() {
            CLASS_MESH_VERTEX
        } else {
            CLASS_MESH
        })
    } else {
        record.sprite.map(|_| CLASS_SPRITE)
    }
}

/// A class's fragment and vertex shaders.
fn variant_of(record: &kh3_shaded::Record, class: u8) -> Option<(usize, Option<usize>)> {
    match class {
        CLASS_SPRITE => record.sprite.map(|v| (v, None)),
        CLASS_MESH => record.mesh.map(|v| (v, None)),
        CLASS_MESH_VERTEX => record.mesh.map(|v| (v, record.mesh_vertex)),
        CLASS_DECAL => record.decal.map(|v| (v, None)),
        CLASS_SPRITE_DISTORT => record.sprite_distort.map(|v| (v, None)),
        CLASS_MESH_DISTORT => record.mesh_distort.map(|v| (v, None)),
        _ => None,
    }
}

fn emitter_code(mode: Blend) -> u8 {
    match mode {
        Blend::Material => 0,
        Blend::Translucent => 1,
        Blend::Additive => 2,
    }
}

fn clamp_colour(c: [f32; 4]) -> [f32; 4] {
    [
        c[0].max(0.),
        c[1].max(0.),
        c[2].max(0.),
        c[3].clamp(0., 1e4),
    ]
}

/// The mesh particle's UV turn and reverse (`uv_mode`), as its vertex shader makes them.
fn turned_uv(uv: [f32; 2], mode: u32) -> [f32; 2] {
    if mode == 0 {
        return uv;
    }
    let a = if mode & 4 != 0 { -uv[0] } else { uv[0] };
    let b = if mode & 8 != 0 { -uv[1] } else { uv[1] };
    match mode & 3 {
        1 => [b, -a],
        2 => [-b, a],
        _ => [a, b],
    }
}

/// Adds a draw's vertices to the translated materials' batches.
fn shaded_quad(
    v: &mut kh3_shaded::ShadedVerts,
    q: &kh3_cascade::Quad,
    eye: glam::Vec3,
    floor: f32,
) {
    let base = v.position.len() as u32;
    let to_camera = (eye - q.center).normalize_or(glam::Vec3::Z);
    let along = (q.corners[1] - q.corners[0]).normalize_or(glam::Vec3::X);
    let (uv, uv2) = (q.uv, q.uv2);
    let corners = [
        [uv[0], uv[1], uv2[0], uv2[1]],
        [uv[2], uv[1], uv2[2], uv2[1]],
        [uv[2], uv[3], uv2[2], uv2[3]],
        [uv[0], uv[3], uv2[0], uv2[3]],
    ];
    for (corner, uv) in q.corners.iter().zip(corners) {
        v.position.push(from_kh3(*corner));
        v.game.push(corner.to_array());
        v.uv.push(uv);
        v.color.push(clamp_colour(q.color));
        v.vcol.push([1.; 4]);
        v.dynamic.push(q.dynamic);
        v.misc.push([q.time, q.rel, q.blend, 1.]);
        v.normal.push(to_camera.to_array());
        v.tangent.push(along.extend(floor).to_array());
        v.particle
            .push([q.center.x, q.center.y, q.center.z, q.size]);
        v.tc1.push(q.dynamic);
    }
    v.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn shaded_ribbon(
    v: &mut kh3_shaded::ShadedVerts,
    r: &kh3_cascade::Ribbon,
    eye: glam::Vec3,
    floor: f32,
) {
    let points = &r.points;
    if points.len() < 2 {
        return;
    }
    // Smoothed with a Catmull-Rom curve between its samples, as the host's ribbons are.
    let spline = |p0: glam::Vec3, p1: glam::Vec3, p2: glam::Vec3, p3: glam::Vec3, t: f32| {
        0.5 * ((2. * p1)
            + (p2 - p0) * t
            + (2. * p0 - 5. * p1 + 4. * p2 - p3) * t * t
            + (3. * p1 - p0 - 3. * p2 + p3) * t * t * t)
    };
    let mut rows = Vec::new();
    let last = points.len() - 1;
    for i in 0..last {
        let (p0, p1, p2, p3) = (
            &points[i.saturating_sub(1)],
            &points[i],
            &points[i + 1],
            &points[(i + 2).min(last)],
        );
        let span = p1.a.distance(p2.a).max(p1.b.distance(p2.b));
        let parts = ((span / 8.).ceil() as usize).clamp(1, 8);
        for k in 0..parts {
            let t = k as f32 / parts as f32;
            let colour = [0, 1, 2, 3].map(|c| p1.color[c] + (p2.color[c] - p1.color[c]) * t);
            rows.push((
                spline(p0.a, p1.a, p2.a, p3.a, t),
                spline(p0.b, p1.b, p2.b, p3.b, t),
                p1.u + (p2.u - p1.u) * t,
                colour,
                p2.a - p1.a,
            ));
        }
    }
    let end = &points[last];
    rows.push((end.a, end.b, end.u, end.color, end.a - points[last - 1].a));
    let mut before: Option<u32> = None;
    for (a, b, u, colour, along) in rows {
        let base = v.position.len() as u32;
        let mid = (a + b) * 0.5;
        let normal = (eye - mid).normalize_or(glam::Vec3::Z);
        let along = along.normalize_or(glam::Vec3::X);
        let u = u * r.tiles;
        let (top, bottom) = if r.uv_turned {
            ([0., u], [r.v_scale, u])
        } else {
            ([u, 0.], [u, r.v_scale])
        };
        for (at, uv) in [(a, top), (b, bottom)] {
            v.position.push(from_kh3(at));
            v.game.push(at.to_array());
            v.uv.push([uv[0], uv[1], uv[0], uv[1]]);
            v.color.push(clamp_colour(colour));
            v.vcol.push([1.; 4]);
            v.dynamic.push(r.dynamic);
            v.misc.push([r.time, 0., 0., 1.]);
            v.normal.push(normal.to_array());
            v.tangent.push(along.extend(floor).to_array());
            v.particle.push([mid.x, mid.y, mid.z, a.distance(b)]);
            v.tc1.push(r.dynamic);
        }
        if let Some(l) = before {
            v.indices.extend([l, l + 1, base, l + 1, base + 1, base]);
        }
        before = Some(base);
    }
}

/// A decal's box, with the world to box rows for the shader.
fn shaded_decal(v: &mut kh3_shaded::ShadedVerts, d: &kh3_cascade::Decal, floor: f32) {
    let m = d.transform;
    if glam::Mat3::from(m.matrix3).determinant().abs() < 1e-9 {
        return;
    }
    let base = v.position.len() as u32;
    let inverse = m.inverse();
    let row = |k: usize| {
        [
            inverse.matrix3.x_axis[k],
            inverse.matrix3.y_axis[k],
            inverse.matrix3.z_axis[k],
            inverse.translation[k],
        ]
    };
    for corner in 0..8u32 {
        let side = |bit: u32| if corner & bit == 0 { -1. } else { 1. };
        let at = m.transform_point3(glam::Vec3::new(side(1), side(2), side(4)));
        v.position.push(from_kh3(at));
        v.game.push(at.to_array());
        v.uv.push([0., 0., d.z_fade[0], d.z_fade[1]]);
        v.color.push(clamp_colour(d.color));
        v.vcol.push(row(0));
        v.dynamic.push(row(1));
        v.misc.push([d.time, d.custom[0], d.custom[1], d.custom[2]]);
        v.normal.push([0., 0., 1.]);
        v.tangent.push([1., 0., 0., floor]);
        v.particle.push(row(2));
        v.tc1.push([0.; 4]);
    }
    const FACES: [[u32; 4]; 6] = [
        [0, 2, 6, 4],
        [1, 5, 7, 3],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 1, 3, 2],
        [4, 6, 7, 5],
    ];
    for f in FACES {
        v.indices
            .extend([f[0], f[1], f[2], f[0], f[2], f[3]].map(|i| base + i));
    }
}

/// A mesh particle for its own translated vertex shader: the mesh's vertices and the instance.
fn shaded_mesh_vertex(
    v: &mut kh3_shaded::ShadedVerts,
    m: &kh3_cascade::MeshInstance,
    mesh: &StaticMesh,
    normals: Option<&Vec<glam::Vec3>>,
    floor: f32,
) {
    let base = v.position.len() as u32;
    let colour = clamp_colour(m.color);
    let t = m.transform;
    let row = |k: usize| {
        [
            t.matrix3.x_axis[k],
            t.matrix3.y_axis[k],
            t.matrix3.z_axis[k],
            t.translation[k],
        ]
    };
    for (i, p) in mesh.positions.iter().enumerate() {
        let uv = mesh.uv.get(i).copied().unwrap_or([0.; 2]);
        v.position
            .push(from_kh3(t.transform_point3(glam::Vec3::from_array(*p))));
        v.game.push(*p);
        v.uv.push([uv[0], uv[1], uv[0], uv[1]]);
        v.color.push(colour);
        v.vcol.push(mesh.colors.get(i).map_or([1.; 4], |c| {
            [
                f32::from(c[0]) / 255.,
                f32::from(c[1]) / 255.,
                f32::from(c[2]) / 255.,
                f32::from(c[3]) / 255.,
            ]
        }));
        v.dynamic.push(m.dynamic);
        v.misc.push([m.time, m.rel, m.uv_mode, m.sub]);
        let n = mesh
            .normals
            .get(i)
            .copied()
            .or_else(|| normals.and_then(|n| n.get(i)).map(|n| n.to_array()))
            .unwrap_or([0., 0., 1.]);
        v.normal.push(n);
        v.tangent
            .push(mesh.tangents.get(i).copied().unwrap_or([1., 0., 0., 1.]));
        v.particle.push([floor, 0., 0., 0.]);
        v.tc1.push([uv[0], uv[1], uv[0], uv[1]]);
        v.rows[0].push(row(0));
        v.rows[1].push(row(1));
        v.rows[2].push(row(2));
    }
    v.indices.extend(mesh.indices.iter().map(|i| i + base));
}

fn shaded_mesh(
    v: &mut kh3_shaded::ShadedVerts,
    m: &kh3_cascade::MeshInstance,
    mesh: &StaticMesh,
    normals: Option<&Vec<glam::Vec3>>,
    floor: f32,
) {
    let base = v.position.len() as u32;
    let origin = glam::Vec3::from(m.transform.translation);
    let scale = glam::Vec3::from(m.transform.matrix3.x_axis).length();
    let colour = clamp_colour(m.color);
    for (i, p) in mesh.positions.iter().enumerate() {
        let world = m.transform.transform_point3(glam::Vec3::from_array(*p));
        let uv = mesh.uv.get(i).copied().unwrap_or([0.; 2]);
        let uv = turned_uv(uv, m.uv_mode as u32);
        let own = mesh
            .normals
            .get(i)
            .map(|n| glam::Vec3::from_array(*n))
            .or_else(|| normals.and_then(|n| n.get(i)).copied())
            .unwrap_or(glam::Vec3::Z);
        // Through the inverse transpose, up to its determinant: the cofactors.
        let cof = m.transform.matrix3;
        let (cx, cy, cz) = (
            cof.y_axis.cross(cof.z_axis),
            cof.z_axis.cross(cof.x_axis),
            cof.x_axis.cross(cof.y_axis),
        );
        let n = (glam::Vec3::from(cx) * own.x
            + glam::Vec3::from(cy) * own.y
            + glam::Vec3::from(cz) * own.z)
            .normalize_or(glam::Vec3::Z);
        let n = if cof.determinant() < 0. { -n } else { n };
        let tangent = mesh
            .tangents
            .get(i)
            .map_or(n.cross(glam::Vec3::Z), |t| {
                m.transform
                    .transform_vector3(glam::Vec3::new(t[0], t[1], t[2]))
            })
            .normalize_or(glam::Vec3::X);
        let sign = mesh.tangents.get(i).map_or(1., |t| t[3]);
        v.position.push(from_kh3(world));
        v.game.push(world.to_array());
        v.uv.push([uv[0], uv[1], uv[0], uv[1]]);
        v.color.push(colour);
        v.vcol.push(mesh.colors.get(i).map_or([1.; 4], |c| {
            [
                f32::from(c[0]) / 255.,
                f32::from(c[1]) / 255.,
                f32::from(c[2]) / 255.,
                f32::from(c[3]) / 255.,
            ]
        }));
        v.dynamic.push(m.dynamic);
        v.misc.push([m.time, m.rel, m.sub, sign]);
        v.normal.push(n.to_array());
        v.tangent.push(tangent.extend(floor).to_array());
        v.particle.push([origin.x, origin.y, origin.z, scale]);
        v.tc1.push([uv[0], uv[1], uv[0], uv[1]]);
    }
    v.indices.extend(mesh.indices.iter().map(|i| i + base));
}

#[allow(clippy::too_many_arguments)]
fn draw_list(
    fx: &mut Kh3Fx,
    eye: glam::Vec3,
    list: &DrawList,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    effect_materials: &mut Assets<EffectMaterial>,
    shaded_materials: &mut Assets<kh3_shaded::ShadedMaterial>,
    visibilities: &mut Query<&mut Visibility>,
) {
    let mut shaded: HashMap<(u32, u8, u8), kh3_shaded::ShadedVerts> = HashMap::new();
    let mut batches: HashMap<(u32, u8), Verts> = HashMap::new();
    let material = |fx: &Kh3Fx, id: u32| {
        fx.materials
            .get(&id)
            .map_or((MatBlend::Additive, [1.; 3]), |m| (m.1, m.2))
    };
    // A shader's masks and colour data with no picture: grey, and a mesh of it faint, since what
    // shapes it in the game is that shader.
    let plain = |fx: &Kh3Fx, id: u32, c: [f32; 4]| {
        if fx.packed_materials.contains(&id) {
            let grey = (c[0] + c[1] + c[2]) / 3.;
            [grey, grey, grey, c[3]]
        } else {
            c
        }
    };
    // Only a mesh can stand in plain: a sprite with no picture would be a square.
    let textured = |fx: &Kh3Fx, id: u32| fx.materials.get(&id).is_some_and(|m| m.0.is_some());
    for q in &list.quads {
        if fx
            .shaded
            .get(&q.material)
            .is_some_and(|r| r.sprite.is_some())
        {
            let key = (q.material, CLASS_SPRITE, emitter_code(q.mode));
            shaded_quad(shaded.entry(key).or_default(), q, eye, fx.floor);
            continue;
        }
        if !textured(fx, q.material) {
            continue;
        }
        let (blend, tint) = material(fx, q.material);
        let v = batches
            .entry((q.material, pass(q.mode, blend)))
            .or_default();
        let c = plain(fx, q.material, gamma(q.color, tint));
        let uv = q.uv;
        let a = v.push(q.corners[0], [uv[0], uv[1]], c);
        let b = v.push(q.corners[1], [uv[2], uv[1]], c);
        let d = v.push(q.corners[2], [uv[2], uv[3]], c);
        let e = v.push(q.corners[3], [uv[0], uv[3]], c);
        v.indices.extend_from_slice(&[a, b, d, a, d, e]);
    }
    for r in &list.ribbons {
        if fx
            .shaded
            .get(&r.material)
            .is_some_and(|x| x.sprite.is_some())
        {
            let key = (r.material, CLASS_SPRITE, emitter_code(r.blend));
            shaded_ribbon(shaded.entry(key).or_default(), r, eye, fx.floor);
            continue;
        }
        if !textured(fx, r.material) {
            continue;
        }
        let (blend, tint) = material(fx, r.material);
        let v = batches
            .entry((r.material, pass(r.blend, blend)))
            .or_default();
        let mut last: Option<(u32, u32)> = None;
        for p in &r.points {
            let c = plain(fx, r.material, gamma(p.color, tint));
            let (u, top, bottom) = if r.uv_turned {
                (r.v_scale, [0., p.u * r.tiles], [r.v_scale, p.u * r.tiles])
            } else {
                (r.v_scale, [p.u * r.tiles, 0.], [p.u * r.tiles, r.v_scale])
            };
            let _ = u;
            let a = v.push(p.a, top, c);
            let b = v.push(p.b, bottom, c);
            if let Some((la, lb)) = last {
                v.indices.extend_from_slice(&[la, lb, a, a, lb, b]);
            }
            last = Some((a, b));
        }
    }
    for m in &list.meshes {
        let own = if fx.shaded.contains_key(&m.material) {
            Some(m.material)
        } else {
            fx.mesh_materials
                .get(&m.mesh)
                .filter(|i| fx.shaded.contains_key(i) && !fx.materials.contains_key(&m.material))
                .copied()
        };
        if let Some(id) = own
            && let Some(class) = fx.shaded.get(&id).and_then(mesh_class)
            && let Some(mesh) = fx.meshes.get(&m.mesh)
        {
            let key = (id, class, emitter_code(m.blend));
            let verts = shaded.entry(key).or_default();
            let normals = fx.mesh_normals.get(&m.mesh);
            if class == CLASS_MESH_VERTEX {
                shaded_mesh_vertex(verts, m, mesh, normals, fx.floor);
            } else {
                shaded_mesh(verts, m, mesh, normals, fx.floor);
            }
            continue;
        }
        let id = if fx.materials.contains_key(&m.material) {
            m.material
        } else {
            match fx
                .mesh_materials
                .get(&m.mesh)
                .filter(|i| fx.materials.contains_key(i))
            {
                Some(i) => *i,
                None => continue,
            }
        };
        let (blend, tint) = material(fx, id);
        let Some(mesh) = fx.meshes.get(&m.mesh) else {
            continue;
        };
        let v = batches.entry((id, pass(m.blend, blend))).or_default();
        let mut c = plain(fx, id, gamma(m.color, tint));
        if fx.packed_materials.contains(&id) {
            c[3] *= PACKED_MESH_ALPHA;
        }
        let rims = fx
            .rim_materials
            .contains(&id)
            .then(|| fx.mesh_normals.get(&m.mesh))
            .flatten();
        let base = v.positions.len() as u32;
        let (flip_u, flip_v) = (m.uv_mode as i32 & 4 != 0, m.uv_mode as i32 & 8 != 0);
        for (i, p) in mesh.positions.iter().enumerate() {
            let world = m.transform.transform_point3(glam::Vec3::from_array(*p));
            let mut uv = mesh.uv.get(i).copied().unwrap_or([0.; 2]);
            if flip_u {
                uv[0] = 1. - uv[0];
            }
            if flip_v {
                uv[1] = 1. - uv[1];
            }
            let mut tone = mesh.colors.get(i).map_or(1., |x| f32::from(x[3]) / 255.);
            if let Some(n) = rims.and_then(|r| r.get(i)) {
                let turned = m.transform.transform_vector3(*n).normalize_or_zero();
                tone *= rim(turned, (eye - world).normalize_or_zero());
            }
            v.push(world, uv, [c[0], c[1], c[2], c[3] * tone]);
        }
        v.indices.extend(mesh.indices.iter().map(|i| i + base));
    }
    for d in &list.decals {
        if fx
            .shaded
            .get(&d.material)
            .is_some_and(|r| r.decal.is_some())
        {
            let key = (d.material, CLASS_DECAL, 0);
            shaded_decal(shaded.entry(key).or_default(), d, fx.floor);
        }
    }
    // A material with a distortion pass draws its geometry again for it.
    let twins: Vec<_> = shaded
        .iter()
        .filter_map(|(key, verts)| {
            let record = fx.shaded.get(&key.0)?;
            let class = match key.1 {
                CLASS_SPRITE if record.sprite_distort.is_some() => CLASS_SPRITE_DISTORT,
                CLASS_MESH if record.mesh_distort.is_some() => CLASS_MESH_DISTORT,
                _ => return None,
            };
            Some(((key.0, class, 0), verts.clone()))
        })
        .collect();
    for (key, verts) in twins {
        shaded.insert(key, verts);
    }
    // The translated materials' draws.
    let clock = fx.clock;
    let mut shaded_shown = HashSet::new();
    for (key, verts) in shaded {
        if verts.indices.is_empty() {
            continue;
        }
        let Some(record) = fx.shaded.get(&key.0).cloned() else {
            continue;
        };
        let Some((variant, vertex)) = variant_of(&record, key.1) else {
            continue;
        };
        shaded_shown.insert(key);
        if !fx.shaded_batches.contains_key(&key) {
            let Some(shared) = fx.shared_samplers.clone() else {
                continue;
            };
            let mesh = meshes.add(Mesh::new(
                bevy::mesh::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::RENDER_WORLD
                    | bevy::asset::RenderAssetUsages::MAIN_WORLD,
            ));
            let plain = key.1 == CLASS_SPRITE || key.1 == CLASS_MESH || key.1 == CLASS_MESH_VERTEX;
            let additive = plain && key.2 == 2 && record.blend == MatBlend::Translucent;
            let reverse = plain && key.2 == 1 && record.blend == MatBlend::Additive;
            let pass = match key.1 {
                CLASS_DECAL if record.decal_adds => Some(6),
                CLASS_DECAL => Some(5),
                CLASS_SPRITE_DISTORT | CLASS_MESH_DISTORT => Some(4),
                _ => None,
            };
            let mut used = fx
                .variant_textures
                .get(variant)
                .cloned()
                .unwrap_or_default();
            if let Some(more) = vertex.and_then(|v| fx.variant_textures.get(v)) {
                used.extend(more.iter().copied());
            }
            let mut asset = kh3_shaded::make_material(
                &record,
                variant,
                vertex,
                pass,
                &used,
                &fx.shaded_images,
                &shared,
                additive,
                reverse,
            );
            // An emissive decal adds emissive x opacity.
            if key.1 == CLASS_DECAL && record.decal_adds {
                asset.params.misc.z = 1.;
            }
            let material = shaded_materials.add(asset);
            let entity = commands
                .spawn((
                    Kh3FxPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::Visible,
                    bevy::camera::visibility::NoFrustumCulling,
                    // Decals and refraction add no light of their own.
                    if plain {
                        bevy::camera::visibility::RenderLayers::from_layers(&[
                            0,
                            super::kh3_post::GLOW_LAYER,
                        ])
                    } else {
                        bevy::camera::visibility::RenderLayers::layer(0)
                    },
                ))
                .id();
            fx.shaded_batches.insert(
                key,
                ShadedBatch {
                    entity,
                    mesh,
                    material,
                },
            );
        }
        let Some(batch) = fx.shaded_batches.get(&key) else {
            continue;
        };
        if let Some(mut mesh) = meshes.get_mut(&batch.mesh) {
            let middle = verts.store(&mut mesh);
            commands
                .entity(batch.entity)
                .insert(Transform::from_translation(middle));
        }
        if !record.timed.is_empty()
            && let Some(mut material) = shaded_materials.get_mut(&batch.material)
        {
            for (at, n, e) in &record.timed {
                let v = kh3_shaded::eval(e, clock, &record);
                for (k, value) in v.iter().enumerate().take(*n) {
                    let place = at + k;
                    if let Some(row) = material.params.m.get_mut(place / 4) {
                        row[place % 4] = *value;
                    }
                }
            }
        }
        if let Ok(mut vis) = visibilities.get_mut(batch.entity) {
            *vis = Visibility::Visible;
        }
    }
    fx.drawing = !shaded_shown.is_empty();
    for (key, batch) in &fx.shaded_batches {
        if !shaded_shown.contains(key)
            && let Ok(mut vis) = visibilities.get_mut(batch.entity)
        {
            *vis = Visibility::Hidden;
        }
    }
    let mut shown = HashSet::new();
    for (key, verts) in batches {
        if verts.indices.is_empty() {
            continue;
        }
        shown.insert(key);
        let batch = fx.batches.entry(key).or_insert_with(|| {
            let mesh = meshes.add(Mesh::new(
                bevy::mesh::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::RENDER_WORLD
                    | bevy::asset::RenderAssetUsages::MAIN_WORLD,
            ));
            let texture = fx.materials.get(&key.0).and_then(|m| m.0.clone());
            let blend = if key.1 == 1 {
                DrawBlend::Add
            } else {
                DrawBlend::Alpha
            };
            let material = effect_materials.add(EffectMaterial::new(blend, texture, 0., true));
            fx.effect_materials.insert(key, material.clone());
            let entity = commands
                .spawn((
                    Kh3FxPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::default(),
                    Visibility::Visible,
                    bevy::camera::visibility::NoFrustumCulling,
                ))
                .id();
            Batch { entity, mesh }
        });
        if let Some(mut mesh) = meshes.get_mut(&batch.mesh) {
            // Gone first: a batch's size changes, and the mesh checks each against the others.
            mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0);
            mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, verts.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, verts.uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, verts.colors);
            mesh.insert_indices(bevy::mesh::Indices::U32(verts.indices));
        }
        if let Ok(mut vis) = visibilities.get_mut(batch.entity) {
            *vis = Visibility::Visible;
        }
    }
    for (key, batch) in &fx.batches {
        if !shown.contains(key)
            && let Ok(mut vis) = visibilities.get_mut(batch.entity)
        {
            *vis = Visibility::Hidden;
        }
    }
}

/// Hooks up what the load thread read.
pub fn begin(
    fx: &mut Kh3Fx,
    data: Arc<FxData>,
    images: &mut Assets<Image>,
    shaders: &mut Assets<Shader>,
) {
    kh3_shaded::register_variants(&data.variants, shaders);
    fx.start(data, images);
}

#[cfg(test)]
mod material_tests {
    use super::*;

    #[test]
    fn the_material_s_own_picture_comes_before_shared_noise() {
        assert_eq!(sampled_rank("ktx_ftb_00m"), Some(0));
        assert_eq!(sampled_rank("jtx_glw_05a"), Some(1));
        assert_eq!(sampled_rank("jtx_dis_00a"), Some(3));
        assert_eq!(sampled_rank("stx_sho_01n"), None);
        assert_eq!(sampled_rank("jtx_msk_00a_clamp"), None);
    }

    #[test]
    fn a_root_s_kind_letters_say_if_it_shows_by_the_rim() {
        for root in [
            "jmb_wht_fsa",
            "tmt_tor_mix00fsa",
            "jmb_2fresrnel_fsa",
            "smt_frn_01a",
        ] {
            assert!(shows_rim(root), "{root}");
        }
        for root in ["jmb_wht_sa", "jmb_wht_a", "kmt_clv_00o", "smt_blz_ksk_00k"] {
            assert!(!shows_rim(root), "{root}");
        }
    }

    #[test]
    fn a_rim_shows_edge_on_and_not_face_on() {
        let eye = glam::Vec3::Z;
        assert!(rim(glam::Vec3::Z, eye) < 1e-6);
        assert!(rim(-glam::Vec3::Z, eye) < 1e-6);
        assert!((rim(glam::Vec3::X, eye) - 1.).abs() < 1e-6);
    }

    #[test]
    fn a_material_that_only_bends_the_scene_is_not_a_shape() {
        let of = |root: &str| {
            fx_material(&MaterialParams {
                root: root.to_owned(),
                ..MaterialParams::default()
            })
        };
        assert!(of("smt_dis_sho_00sa").texture.is_none() && !of("smt_dis_sho_00sa").flat);
        assert!(of("jmb_wht_a").flat);
    }

    #[test]
    fn normals_point_off_a_face() {
        let mesh = StaticMesh {
            positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            uv: Vec::new(),
            colors: Vec::new(),
            indices: vec![0, 1, 2],
            sections: Vec::new(),
            materials: Vec::new(),
            ..StaticMesh::default()
        };
        assert_eq!(smooth_normals(&mesh), vec![glam::Vec3::Z; 3]);
    }
}
