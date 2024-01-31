use bevy::{
    prelude::{debug, Commands, Entity, Query, Res, ResMut, Resource},
    utils::HashMap,
};

use crate::{CustomReplacableResource, ReplacableComponent};

#[derive(Resource, Default)]
pub struct ReplacableResourceStore {
    map: HashMap<String, Vec<u8>>,
}

pub fn serialize_replacable_resource<R: CustomReplacableResource>(
    mut store: ResMut<ReplacableResourceStore>,
    resource: Option<Res<R>>,
    mut commands: Commands,
) {
    let Some(resource) = resource else {
        return;
    };
    if let Ok(v) = resource.to_vec() {
        store.map.insert(R::get_type_name().to_string(), v);
    }

    commands.remove_resource::<R>();
}

pub fn deserialize_replacable_resource<R: CustomReplacableResource>(
    store: Res<ReplacableResourceStore>,
    mut commands: Commands,
) {
    let name = R::get_type_name();
    debug!("Deserializing {name}");
    let v: R = store
        .map
        .get(name)
        .and_then(|v| R::from_slice(v).ok())
        .unwrap_or_default();

    commands.insert_resource(v);
}

#[derive(Resource, Default)]
pub struct ReplacableComponentStore {
    map: HashMap<String, Vec<(Entity, Vec<u8>)>>,
}

pub fn serialize_replacable_component<C: ReplacableComponent>(
    mut store: ResMut<ReplacableComponentStore>,
    query: Query<(Entity, &C)>,
    mut commands: Commands,
) {
    let name = C::get_type_name();
    for (entity, component) in query.iter() {
        if let Ok(v) = rmp_serde::to_vec(component) {
            let storage = store.map.entry(name.to_string()).or_default();
            storage.push((entity, v));
        }

        commands.entity(entity).remove::<C>();
    }
}

pub fn deserialize_replacable_component<C: ReplacableComponent>(
    mut store: ResMut<ReplacableComponentStore>,
    mut commands: Commands,
) {
    let name = C::get_type_name();
    debug!("Deserializing {name}");

    if let Some(storage) = store.map.remove(name) {
        for (entity, value) in storage.into_iter() {
            let v: C = rmp_serde::from_slice(&value).ok().unwrap_or_default();
            commands.entity(entity).insert(v);
        }
    }
}
