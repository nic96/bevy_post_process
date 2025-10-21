use std::f32::consts::PI;
use bevy::render::render_graph::RenderLabel;
use bevy::{
    prelude::*,
    render::{extract_component::ExtractComponent, render_resource::*},
};
use bevy_post_process_util::PostProcessPlugin;

const SHADER_ASSET_PATH: &str = "shaders/sky.wgsl";

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SkyPipelineLabel;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PostProcessPlugin::<SkySettings, SkyPipelineLabel>::new(
                SHADER_ASSET_PATH,
                SkyPipelineLabel,
                Some("sky_pipeline"),
                "sky_bind_group_layout",
            ),
        ))
        .add_systems(Startup, setup)
        .run();
}

// This is the component that will get passed to the shader
#[derive(Component, Default, Clone, Copy, ExtractComponent, ShaderType)]
struct SkySettings {
    time_of_day: f32,
    sun_rotation: Vec4,
    moon_rotation: Vec4,
}

/// Set up a simple 3D scene
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0.0, 0.0, 5.0)).looking_at(Vec3::default(), Vec3::Y),
        Camera {
            clear_color: Color::NONE.into(),
            ..default()
        },
        // Add the setting to the camera.
        // This component is also used to determine on which camera to run the post processing effect.
        SkySettings { ..default() },
    ));

    // cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(mats.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(0.0, 0.5, 0.0).with_rotation(Quat::from_euler(EulerRot::XYZ, PI / 4., PI / 4., 0.)),
    ));
    // light
    commands.spawn(DirectionalLight {
        illuminance: 1_000.,
        ..default()
    });
}
