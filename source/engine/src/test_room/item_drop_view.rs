//! The treasure boxes of dropped items: the object of the item's box style
//! (`player::enemy_prizes::box_style`, objects 0x3b8..0x3bb = the yellow,
//! red, blue and green `PRIZE_BOX_*` models), shown at the drop's place
//! until it is picked up.
use super::*;

const BOXES: [(u32, &str); 4] = [
    (0x3b8, "PRIZE_BOX_YELLOW"),
    (0x3b9, "PRIZE_BOX_RED"),
    (0x3ba, "PRIZE_BOX_BLUE"),
    (0x3bb, "PRIZE_BOX_GREEN"),
];

#[derive(Resource, Default)]
pub struct DropView {
    models: HashMap<u32, model_inspection::InspectionModel>,
    shown: HashMap<u32, Entity>,
}

/// Reads the four box models from the installed packages; a box that fails
/// to load is simply not drawn.
pub fn load(source: &ProbeSource) -> DropView {
    let mut view = DropView::default();
    let Ok(library) = data::path_lookup::Library::open(source) else {
        return view;
    };
    for (object, name) in BOXES {
        let Some(found) = library.find(&format!("obj/{name}.mdlx")) else {
            continue;
        };
        if let Ok(mut models) =
            model_inspection::read_model_at(source, found.index, found.ordinal, found.name_hash)
            && !models.is_empty()
        {
            view.models.insert(object, models.remove(0));
        }
    }
    view
}

/// Shows a box for every live item drop and removes the picked up ones.
#[allow(clippy::too_many_arguments)]
pub fn present(
    mut commands: Commands,
    game: Res<Game>,
    view: Option<ResMut<DropView>>,
    scene: Option<Res<SceneRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FogMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut transforms: Query<&mut Transform>,
) {
    let (Some(mut view), Some(scene)) = (view, scene) else {
        return;
    };
    let drops = game
        .enemy
        .as_ref()
        .map(|e| e.drops.as_slice())
        .unwrap_or(&[]);
    let DropView { models, shown } = &mut *view;
    shown.retain(|id, entity| {
        let live = drops.iter().any(|d| d.id == *id);
        if !live {
            commands.entity(*entity).despawn();
        }
        live
    });
    for d in drops {
        let transform = Transform::from_translation(Vec3::from_array(d.position))
            .with_rotation(Quat::from_rotation_y(d.yaw))
            .with_scale(Vec3::splat(d.scale));
        if let Some(&entity) = shown.get(&d.id) {
            if let Ok(mut t) = transforms.get_mut(entity) {
                *t = transform;
            }
            continue;
        }
        let Some(model) = models.get(&d.object) else {
            continue;
        };
        let Some(pose) = model.poses.first() else {
            continue;
        };
        let positions: Vec<Vec3> = pose
            .position_bits
            .iter()
            .map(|p| Vec3::from_array(p.map(f32::from_bits)))
            .collect();
        let root = commands
            .spawn((ChildOf(scene.0), transform, Visibility::default()))
            .id();
        spawn_model(
            &mut commands,
            model,
            &positions,
            root,
            &mut meshes,
            &mut materials,
            &mut images,
        );
        shown.insert(d.id, root);
    }
}
