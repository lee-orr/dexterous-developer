use bevy::prelude::{Startup, Update};
use dexterous_developer::*;

fn update() {
    println!("Ran Update");
}

fn startup() {
    println!("Press Enter to Progress, or type 'exit' to exit");
}

#[dexterous_developer_setup]
pub fn reloadable(app: &mut ReloadableAppContents) {
    app.add_systems(Startup, startup)
        .add_systems(Update, update);
}
