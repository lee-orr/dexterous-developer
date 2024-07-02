use std::num::NonZero;

use bevy::{
    app::{AppExit, Startup, Update},
    prelude::*,
    MinimalPlugins,
};
use bevy_dexterous_developer::*;

fn terminal_runner(mut app: App) -> AppExit {
    app.update();
    eprintln!("Ready for Input");
    for line in std::io::stdin().lines() {
        let typed: String = line.unwrap_or_default();
        if typed == "exit" {
            println!("Exiting");
            return AppExit::Success;
        }
        app.update();
    }
    AppExit::Error(NonZero::<u8>::new(1).unwrap())
}

#[derive(States, Debug, Default, Hash, PartialEq, Eq, Clone)]
enum MyState {
    #[default]
    Initial,
    Another,
}

impl ReplacableType for MyState {
    fn get_type_name() -> &'static str {
        "MySerializableResource"
    }

    fn to_vec(&self) -> bevy_dexterous_developer::Result<Vec<u8>> {
        let value = match self {
            MyState::Initial => [0],
            MyState::Another => [1],
        };
        Ok(value.to_vec())
    }

    fn from_slice(val: &[u8]) -> bevy_dexterous_developer::Result<Self> {
        let value = if let Some(val) = val.first() {
            if *val == 1 {
                MyState::Another
            } else {
                MyState::Initial
            }
        } else {
            MyState::Initial
        };
        Ok(value)
    }
}

#[derive(SubStates, Debug, Default, Hash, PartialEq, Eq, Clone)]
#[source(MyState = MyState::Another)]
enum MySubState {
    #[default]
    Initial,
    Another,
}

impl ReplacableType for MySubState {
    fn get_type_name() -> &'static str {
        "MySubState"
    }

    fn to_vec(&self) -> bevy_dexterous_developer::Result<Vec<u8>> {
        let value = match self {
            MySubState::Initial => [0],
            MySubState::Another => [1],
        };
        Ok(value.to_vec())
    }

    fn from_slice(val: &[u8]) -> bevy_dexterous_developer::Result<Self> {
        let value = if let Some(val) = val.first() {
            if *val == 1 {
                MySubState::Another
            } else {
                MySubState::Initial
            }
        } else {
            MySubState::Initial
        };
        Ok(value)
    }
}

reloadable_main!( bevy_main(initial_plugins) {
    App::new()
        .add_plugins(initial_plugins.initialize::<MinimalPlugins>())
        .set_runner(terminal_runner)
        .setup_reloadable_elements::<reloadable>()
        .run();
});

fn set_next_state(mut next_state: ResMut<NextState<MyState>>) {
    println!("In Initial State");
    next_state.set(MyState::Another);
}

fn set_next_sub_state(mut next_state: ResMut<NextState<MySubState>>) {
    println!("In Initial Sub State");
    next_state.set(MySubState::Another);
}

fn in_another_sub_state() {
    println!("In Another Sub State");
}

fn in_another_state() {
    println!("In Another State");
}

fn startup() {
    println!("Press Enter to Progress, or type 'exit' to exit");
}

reloadable_scope!(reloadable(app) {
    app
        .add_systems(Startup, startup)
        .add_systems(Update, (
            set_next_state.run_if(in_state(MyState::Initial)),
            in_another_state.run_if(in_state(MyState::Another)),
            set_next_sub_state.run_if(in_state(MySubState::Initial)),
            in_another_sub_state.run_if(in_state(MySubState::Another)),
        ).chain())
        .init_state::<MyState>()
        .add_sub_state::<MySubState>();
});
