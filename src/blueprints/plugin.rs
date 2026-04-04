use bevy::prelude::*;

use crate::blueprints::{build_registry, build_visual_cache};

pub struct BlueprintPlugin;

impl Plugin for BlueprintPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_blueprints);
    }
}

fn setup_blueprints(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let registry = build_registry();
    let cache = build_visual_cache(&registry, &mut meshes, &mut materials);
    commands.insert_resource(registry);
    commands.insert_resource(cache);
}
