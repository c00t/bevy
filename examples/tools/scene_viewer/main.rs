//! A simple glTF scene viewer made with Bevy.
//!
//! Just run `cargo run --release --example scene_viewer /path/to/model.gltf`,
//! replacing the path as appropriate.
//! In case of multiple scenes, you can select which to display by adapting the file path: `/path/to/model.gltf#Scene1`.
//! With no arguments it will load the `FlightHelmet` glTF model from the repository assets subdirectory.
//!
//! If you want to hot reload asset changes, enable the `file_watcher` cargo feature.

use bevy::{
    log::tracing,
    prelude::*,
    render::primitives::{Aabb, Sphere},
};

#[cfg(feature = "bevy_dev_tools")]
use bevy::{
    dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin},
    text::{FontSmoothing, LineHeight},
};

#[path = "../../helpers/camera_controller.rs"]
mod camera_controller;

#[cfg(feature = "animation")]
mod animation_plugin;
mod morph_viewer_plugin;
mod scene_viewer_plugin;

use bevy_render::view::VisibilityRange;
use camera_controller::{CameraController, CameraControllerPlugin};
use morph_viewer_plugin::MorphViewerPlugin;
use scene_viewer_plugin::{SceneHandle, SceneViewerPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "bevy scene viewer".to_string(),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
                ..default()
            }),
        CameraControllerPlugin,
        SceneViewerPlugin,
        MorphViewerPlugin,
        #[cfg(feature = "bevy_dev_tools")]
        FpsOverlayPlugin {
            config: FpsOverlayConfig {
                text_config: TextFont {
                    // Here we define size of our overlay
                    font_size: 42.0,
                    // If we want, we can use a custom font
                    font: default(),
                    // We could also disable font smoothing,
                    font_smoothing: FontSmoothing::default(),
                    line_height: LineHeight::default(),
                },
                // We can also change color of the overlay
                text_color: Color::srgb(0.0, 1.0, 0.0),
                enabled: true,
            },
        },
    ))
    .add_systems(Startup, (setup,).chain())
    .add_systems(PreUpdate, setup_scene_after_load)
    .add_systems(PostUpdate, debug_visibility);

    #[cfg(feature = "animation")]
    app.add_plugins(animation_plugin::AnimationManipulationPlugin);

    app.run();
}

fn parse_scene(scene_path: String) -> (String, usize) {
    if scene_path.contains('#') {
        let gltf_and_scene = scene_path.split('#').collect::<Vec<_>>();
        if let Some((last, path)) = gltf_and_scene.split_last() {
            if let Some(index) = last
                .strip_prefix("Scene")
                .and_then(|index| index.parse::<usize>().ok())
            {
                return (path.join("#"), index);
            }
        }
    }
    (scene_path, 0)
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let scene_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/models/animated/MorphStressTest.gltf".to_string());
    info!("Loading {}", scene_path);
    let (file_path, scene_index) = parse_scene(scene_path);

    commands.insert_resource(SceneHandle::new(asset_server.load(file_path), scene_index));
}

fn setup_scene_after_load(
    mut commands: Commands,
    mut setup: Local<bool>,
    mut scene_handle: ResMut<SceneHandle>,
    asset_server: Res<AssetServer>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>), With<Mesh3d>>,
) {
    if scene_handle.is_loaded && !*setup {
        *setup = true;
        // Find an approximate bounding box of the scene from its meshes
        if meshes.iter().any(|(_, maybe_aabb)| maybe_aabb.is_none()) {
            info!("meshes don't have aabb");
            return;
        }

        let mut min = Vec3A::splat(f32::MAX);
        let mut max = Vec3A::splat(f32::MIN);
        for (transform, maybe_aabb) in &meshes {
            let aabb = maybe_aabb.unwrap();
            // If the Aabb had not been rotated, applying the non-uniform scale would produce the
            // correct bounds. However, it could very well be rotated and so we first convert to
            // a Sphere, and then back to an Aabb to find the conservative min and max points.
            let sphere = Sphere {
                center: Vec3A::from(transform.transform_point(Vec3::from(aabb.center))),
                radius: transform.radius_vec3a(aabb.half_extents),
            };
            let aabb = Aabb::from(sphere);
            min = min.min(aabb.min());
            max = max.max(aabb.max());
        }

        let size = (max - min).length();
        let aabb = Aabb::from_min_max(Vec3::from(min), Vec3::from(max));

        info!("Spawning a controllable 3D perspective camera");
        let mut projection = PerspectiveProjection::default();
        projection.far = projection.far.max(size * 10.0);

        let walk_speed = size * 0.1;
        let camera_controller = CameraController {
            walk_speed,
            run_speed: 0.1 * walk_speed,
            ..default()
        };

        // Display the controls of the scene viewer
        info!("{}", camera_controller);
        info!("{}", *scene_handle);

        info!(
            "camera position: {}, look at: {}",
            Vec3::from(aabb.center) + size * Vec3::new(0.5, 0.25, 0.5),
            Vec3::from(aabb.center)
        );
        commands.spawn((
            Camera3d::default(),
            Projection::from(projection),
            Transform::from_translation(Vec3::from(aabb.center) + size * Vec3::new(0.5, 0.25, 0.5))
                .looking_at(Vec3::from(aabb.center), Vec3::Y),
            Camera {
                is_active: false,
                ..default()
            },
            EnvironmentMapLight {
                diffuse_map: asset_server
                    .load("assets/environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
                specular_map: asset_server
                    .load("assets/environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
                intensity: 150.0,
                ..default()
            },
            camera_controller,
        ));

        // Spawn a default light if the scene does not have one
        if !scene_handle.has_light {
            info!("Spawning a directional light");
            commands.spawn((
                DirectionalLight::default(),
                Transform::from_xyz(1.0, 1.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
            ));

            scene_handle.has_light = true;
        }
    }
}

// Add this to your system to monitor visibility changes
fn debug_visibility(
    query: Query<(Entity, &ViewVisibility, &VisibilityRange), Changed<ViewVisibility>>,
) {
    for (entity, visibility, range) in &query {
        println!(
            "Entity {:?} visibility changed: {:?}",
            entity,
            visibility.get()
        );
    }
}
