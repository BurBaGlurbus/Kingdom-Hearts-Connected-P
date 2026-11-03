//! What KH3's renderer does to the whole picture for its effects: UE4's glow around the light they
//! add, and the passes they ask for (radial blurs, light streaks, colour filters). The room's frame
//! is a stack of images, so a pass is more images over the scene: a blur is the scene again at each of
//! its taps, in cells that carry its ring's weight. The glow comes from a second camera that draws
//! only the effects and only their light past white. The tap and ring maths are `kh3_cascade::screen`.
use super::kh3_fx::{Kh3Fx, from_kh3};
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::NoIndirectDrawing;
use data::sprite_sequence::Quad;
use engine::test_room::{Compose, EffectGlowCamera, MenuCamera, SnapshotCamera};
use kh3_cascade::screen::{RADIAL_TAPS, ScreenFx, ScreenKind, radial_step, ring_weight};
use visual::overlay::{CENTRE_X, HEIGHT, Ui2dFrame, WIDE_WIDTH};

/// The layer of what the glow camera draws.
pub const GLOW_LAYER: usize = 7;
/// UE4's six stages together, as `kh3_cascade::screen`'s tints and intensity add up.
const GLOW_INTENSITY: f32 = 0.09;
const GRID: (usize, usize) = (32, 18);

#[derive(Resource, Default)]
pub struct Kh3Post {
    image: Option<Handle<Image>>,
    sheet: Option<usize>,
    white: Option<usize>,
}

fn glow_image(size: [u32; 2]) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

/// A place on the screen (UV) in the frame's layout.
fn layout(u: f32, v: f32) -> [f32; 2] {
    [CENTRE_X - WIDE_WIDTH / 2. + WIDE_WIDTH * u, HEIGHT * v]
}

fn whole(sheet_size: [u32; 2], colour: [f32; 4], blend: i32) -> Quad {
    Quad {
        corners: [
            layout(0., 0.),
            layout(1., 0.),
            layout(0., 1.),
            layout(1., 1.),
        ],
        uv: [0, 0, sheet_size[0] as i32, sheet_size[1] as i32],
        colors: [colour; 4],
        blend,
    }
}

/// A blur's taps, far to near, as cells of `sheet`. `add`: added (light streaks), each tap at its own
/// weight; otherwise over the picture, each as much as leaves the weights the shader gives them.
fn taps(
    fx: &ScreenFx,
    center: [f32; 2],
    sheet_size: [u32; 2],
    aspect: f32,
    add: bool,
) -> Vec<Quad> {
    let mut quads = Vec::new();
    let first = usize::from(!add);
    let weight = |u: f32, v: f32| {
        let (dx, dy) = (u - center[0], (v - center[1]) / aspect);
        ring_weight((dx * dx + dy * dy).sqrt(), fx.ring) * fx.alpha.abs()
    };
    // The over-draws from the last one back: each takes its share of what those above it leave.
    let share = |m: f32, tap: usize| {
        if add {
            return m * RADIAL_TAPS[tap];
        }
        let mut left = 1.;
        for w in &RADIAL_TAPS[first..tap] {
            left *= 1. - (m * w / f32::max(left, 1e-3)).min(1.);
        }
        (m * RADIAL_TAPS[tap] / f32::max(left, 1e-3)).min(1.)
    };
    let texel = |u: f32, v: f32, tap: usize| {
        let s = radial_step([u, v], center, fx.power, fx.linear, fx.angle);
        [
            ((u + s[0] * tap as f32) * sheet_size[0] as f32).round() as i32,
            ((v + s[1] * tap as f32) * sheet_size[1] as f32).round() as i32,
        ]
    };
    for tap in (first..RADIAL_TAPS.len()).rev() {
        for j in 0..GRID.1 {
            for i in 0..GRID.0 {
                let (u0, u1) = (i as f32 / GRID.0 as f32, (i + 1) as f32 / GRID.0 as f32);
                let (v0, v1) = (j as f32 / GRID.1 as f32, (j + 1) as f32 / GRID.1 as f32);
                let corners = [(u0, v0), (u1, v0), (u0, v1), (u1, v1)];
                let m = corners.map(|(u, v)| weight(u, v));
                if m.iter().all(|x| *x < 1e-3) {
                    continue;
                }
                let (a, b) = (texel(u0, v0, tap), texel(u1, v1, tap));
                quads.push(Quad {
                    corners: corners.map(|(u, v)| layout(u, v)),
                    uv: [a[0], a[1], b[0], b[1]],
                    colors: m.map(|m| [1., 1., 1., share(m, tap)]),
                    blend: i32::from(add),
                });
            }
        }
    }
    quads
}

/// The glow camera follows the scene's; the passes go over the scene, under the HUD.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    mut commands: Commands,
    mut post: ResMut<Kh3Post>,
    fx: Res<Kh3Fx>,
    compose: Option<Res<Compose>>,
    mut frame: ResMut<Ui2dFrame>,
    mut images: ResMut<Assets<Image>>,
    scene: Query<
        (&Camera, &GlobalTransform, &Transform, &Projection),
        (
            With<Camera3d>,
            Without<SnapshotCamera>,
            Without<MenuCamera>,
            Without<EffectGlowCamera>,
        ),
    >,
    mut glow: Query<(&mut Camera, &mut Transform, &mut Projection), With<EffectGlowCamera>>,
) {
    let Some(compose) = compose else { return };
    let Some((camera, global, transform, projection)) = scene.iter().find(|c| c.0.is_active) else {
        return;
    };
    let sheet_size = frame.sheets[compose.scene_sheet].size;
    let half = sheet_size.map(|v| (v / 2).max(1));
    if !fx.is_drawing() {
        if let Ok((mut c, ..)) = glow.single_mut() {
            c.is_active = false;
        }
        return;
    }
    let Some(handle) = post.image.clone().or_else(|| {
        let handle = images.add(glow_image(half));
        post.image = Some(handle.clone());
        post.sheet = Some(frame.add_image(handle.clone(), half, false));
        commands.spawn((
            EffectGlowCamera,
            Camera3d::default(),
            Camera {
                order: -3,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Image(handle.clone().into()),
            bevy::camera::Hdr,
            Bloom {
                intensity: GLOW_INTENSITY,
                low_frequency_boost: 0.,
                high_pass_frequency: 1.,
                composite_mode: BloomCompositeMode::Additive,
                ..Bloom::NATURAL
            },
            Tonemapping::None,
            DebandDither::Disabled,
            Msaa::Off,
            NoIndirectDrawing,
            RenderLayers::layer(GLOW_LAYER),
            *transform,
            projection.clone(),
        ));
        Some(handle)
    }) else {
        return;
    };
    let Some(glow_sheet) = post.sheet else { return };
    if frame.sheets[glow_sheet].size != half {
        if let Some(mut image) = images.get_mut(&handle) {
            image.resize(Extent3d {
                width: half[0],
                height: half[1],
                depth_or_array_layers: 1,
            });
        }
        frame.sheets[glow_sheet].size = half;
    }
    if let Ok((mut c, mut t, mut p)) = glow.single_mut() {
        c.is_active = true;
        *t = *transform;
        *p = projection.clone();
    }
    let white = *post
        .white
        .get_or_insert_with(|| frame.add_sheet(&mut images, vec![255; 4], [1, 1], 1.));
    let mut layers: Vec<(usize, Vec<Quad>)> = Vec::new();
    let aspect = sheet_size[0] as f32 / sheet_size[1].max(1) as f32;
    let centre = |pass: &ScreenFx| {
        pass.world.map_or(pass.screen, |p| {
            camera
                .world_to_ndc(global, Vec3::from_array(from_kh3(p)))
                .map_or([0.5, 0.5], |n| [(n.x + 1.) * 0.5, (1. - n.y) * 0.5])
        })
    };
    // KH3's order: blurs on the picture, the glow added after them, filters last.
    for pass in fx.screen_passes() {
        if pass.kind == ScreenKind::RadialBlur {
            let quads = taps(pass, centre(pass), sheet_size, aspect, false);
            layers.push((compose.scene_sheet, quads));
        }
    }
    layers.push((glow_sheet, vec![whole(half, [1.; 4], 1)]));
    for pass in fx.screen_passes() {
        if pass.kind == ScreenKind::BloomRadialBlur {
            let quads = taps(pass, centre(pass), half, aspect, true);
            layers.push((glow_sheet, quads));
        }
    }
    for pass in fx.screen_passes() {
        if pass.kind == ScreenKind::Filter && pass.mode <= 1 {
            let big = pass.color.iter().fold(1f32, |a, b| a.max(*b));
            let c = pass.color.map(|v| (v / big).max(0.));
            let a = (pass.alpha * big).clamp(0., 1.);
            let blend = if pass.mode == 0 { 1 } else { 0 };
            layers.push((white, vec![whole([1, 1], [c[0], c[1], c[2], a], blend)]));
        }
    }
    let at = frame
        .draws
        .iter()
        .position(|d| d.0 == compose.scene_sheet)
        .map_or(0, |i| i + 1);
    frame.draws.splice(at..at, layers);
}
