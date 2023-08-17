use bevy::{
    prelude::{Commands, EventWriter, Res, ResMut, Schedule, Schedules, World},
    utils::Instant,
};

use crate::{
    hot::{
        hot_reload_internal::InternalHotReload, update_lib, CleanupReloaded,
        DeserializeReloadables, HotReload, HotReloadEvent, ReloadableAppCleanupData,
        ReloadableAppContents, ReloadableSchedule, ReloadableSetup, SerializeReloadables,
        SetupReload,
    },
    OnReloadComplete,
};

pub fn update_lib_system(
    mut dexterous_developer_int: ResMut<InternalHotReload>,
    mut dexterous_developer: ResMut<HotReload>,
    mut event: EventWriter<HotReloadEvent>,
) {
    dexterous_developer_int.updated_this_frame = false;
    dexterous_developer.updated_this_frame = false;

    if let Some(lib) = update_lib::update_lib(&dexterous_developer_int.libs) {
        println!("Got Update");
        dexterous_developer_int.last_lib = dexterous_developer_int.library.clone();
        dexterous_developer_int.library = Some(lib);
        dexterous_developer_int.updated_this_frame = true;
        dexterous_developer.updated_this_frame = true;
        dexterous_developer_int.last_update_time = Instant::now();
    }

    dexterous_developer.updated_this_frame = dexterous_developer_int.updated_this_frame;
    event.send(HotReloadEvent {
        last_update_time: dexterous_developer_int.last_update_time,
    });
}

pub fn reload(world: &mut World) {
    let internal_state = world.resource::<InternalHotReload>();
    if !internal_state.updated_this_frame {
        return;
    }
    println!("Serializing...");
    let _ = world.try_run_schedule(SerializeReloadables);
    println!("Cleanup...");
    let _ = world.try_run_schedule(CleanupReloaded);
    println!("Setup...");
    let _ = world.try_run_schedule(SetupReload);
    println!("Set Schedules...");
    register_schedules(world);
    println!("Deserialize...");
    let _ = world.try_run_schedule(DeserializeReloadables);
    println!("reload complete");
    let _ = world.try_run_schedule(OnReloadComplete);
}

pub fn setup_reloadable_app<T: ReloadableSetup>(name: &'static str, world: &mut World) {
    println!("Setting up reloadables at {name}");
    let Some(internal_state) = world.get_resource::<InternalHotReload>() else {
        return;
    };

    println!("got internal reload state");

    let Some(lib) = &internal_state.library else {
        println!("can't get library");
        let Some(mut reloadable) = world.get_resource_mut::<ReloadableAppContents>() else {
            return;
        };
        println!("setup default");

        T::default_function(reloadable.as_mut());
        return;
    };
    let lib = lib.clone();
    let Some(lib) = lib.library() else {
        println!("can't access library internals ");
        let Some(mut reloadable) = world.get_resource_mut::<ReloadableAppContents>() else {
            return;
        };
        println!("setup default");
        T::default_function(reloadable.as_mut());
        return;
    };

    let Some(mut reloadable) = world.get_resource_mut::<ReloadableAppContents>() else {
        println!("no reloadable app");
        return;
    };
    unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn(&mut ReloadableAppContents)> = lib
            .get(name.as_bytes())
            .unwrap_or_else(|_| panic!("Can't find reloadable setup function",));
        func(&mut reloadable)
    };
    println!("setup for {name} complete");
}

pub fn register_schedules(world: &mut World) {
    println!("Reloading schedules");
    let Some(reloadable) = world.remove_resource::<ReloadableAppContents>() else {
        return;
    };
    println!("Has reloadable app");

    let Some(mut schedules) = world.get_resource_mut::<Schedules>() else {
        return;
    };

    println!("Has schedules resource");

    let mut inner = ReloadableAppCleanupData::default();

    for (original, schedule) in reloadable.schedule_iter() {
        let label = ReloadableSchedule::new(original.clone());
        println!("Adding {label:?} to schedule");
        inner.labels.insert(Box::new(label.clone()));
        let exists = schedules.insert(label.clone(), schedule);
        if exists.is_none() {
            if let Some(root) = schedules.get_mut(&original) {
                let label = label.clone();
                root.add_systems(move |w: &mut World| {
                    let _ = w.try_run_schedule(label.clone());
                });
            } else {
                let label = label.clone();
                let mut root = Schedule::new();
                root.add_systems(move |w: &mut World| {
                    let _ = w.try_run_schedule(label.clone());
                });
                schedules.insert(original, root);
            }
        }
    }

    world.insert_resource(inner);
}

pub fn cleanup(
    mut commands: Commands,
    mut schedules: ResMut<Schedules>,
    reloadable: Res<ReloadableAppCleanupData>,
) {
    for schedule in reloadable.labels.iter() {
        println!("Attempting cleanup for {schedule:?}");
        let cleadn = schedules.insert(schedule.clone(), Schedule::default());
        println!(
            "Tried cleaning {schedule:?} was empty: {}",
            cleadn.is_none()
        );
    }
    println!("Cleanup almost complete");

    commands.insert_resource(ReloadableAppContents::default());
    println!("Cleanup complete");
}

pub fn dexterous_developer_occured(reload: Res<InternalHotReload>) -> bool {
    reload.updated_this_frame
}