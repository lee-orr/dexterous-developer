use bevy::{
    ecs::{event::EventRegistry, schedule::ScheduleLabel},
    prelude::*,
    state::{state::FreelyMutableState, state_scoped::clear_state_scoped_entities},
    utils::{HashMap, HashSet},
};

use super::{super::types::*, reload_systems::dexterous_developer_occured};

use super::{replacable_types::*, schedules::*};

#[derive(Default, Resource, Clone, Debug)]
pub struct ReloadableAppCleanupData {
    pub labels: HashSet<ReloadableSchedule<WrappedSchedule>>,
}

#[derive(Default, Resource)]
pub struct ReloadableAppElements {
    schedules: HashMap<WrappedSchedule, (Schedule, ReloadableSchedule<WrappedSchedule>)>,
    resources: HashSet<String>,
    components: HashSet<String>,
    run_once: HashSet<String>,
}

impl ReloadableAppElements {
    pub fn schedule_iter(
        self,
    ) -> impl Iterator<
        Item = (
            WrappedSchedule,
            Schedule,
            ReloadableSchedule<WrappedSchedule>,
        ),
    > {
        self.schedules.into_iter().map(|(a, (b, c))| (a, b, c))
    }
}

pub struct ReloadableAppContents<'a> {
    name: &'static str,
    schedules: &'a mut HashMap<WrappedSchedule, (Schedule, ReloadableSchedule<WrappedSchedule>)>,
    resources: &'a mut HashSet<String>,
    components: &'a mut HashSet<String>,
    run_once: &'a mut HashSet<String>,
}

impl<'a> ReloadableAppContents<'a> {
    pub fn new(name: &'static str, elements: &'a mut ReloadableAppElements) -> Self {
        Self {
            name,
            schedules: &mut elements.schedules,
            resources: &mut elements.resources,
            components: &mut elements.components,
            run_once: &mut elements.run_once,
        }
    }

    fn run_only_on_first_load<M>(
        &mut self,
        name: &'static str,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        if self.run_once.insert(name.to_string()) {
            self.add_systems(OnReloadComplete, systems);
        }
        self
    }
}

impl<'a> crate::private::ReloadableAppSealed for ReloadableAppContents<'a> {}

impl<'a> crate::ReloadableApp for ReloadableAppContents<'a> {
    fn add_systems<M, L: ScheduleLabel + Eq + ::std::hash::Hash + Clone>(
        &mut self,
        schedule: L,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        trace!("Adding To Schedule {schedule:?}");
        let schedules = &mut self.schedules;
        let wrapped: WrappedSchedule = WrappedSchedule::new(schedule.clone());

        if let Some((schedule, _)) = schedules.get_mut(&wrapped) {
            trace!("Adding systems to schedule");
            schedule.add_systems(systems);
        } else {
            trace!("Creating schedule with systems");
            let reloadable = ReloadableSchedule::new(wrapped.clone());
            let mut new_schedule = Schedule::new(reloadable.clone());
            new_schedule.add_systems(systems);
            schedules.insert(wrapped, (new_schedule, reloadable));
        }
        trace!("Adding To Schedule Complete");

        self
    }

    fn register_serializable_resource<R: Resource + ReplacableType>(&mut self) -> &mut Self {
        let name = R::get_type_name();
        if !self.resources.contains(name) {
            self.resources.insert(name.to_string());
            trace!("adding resource {name}");
            let reloadable_element_name = self.name;
            self.add_systems(
                SerializeReloadables,
                serialize_replacable_resource::<R>
                    .run_if(element_selection_condition(reloadable_element_name)),
            )
            .add_systems(
                DeserializeReloadables,
                deserialize_replacable_resource::<R>
                    .run_if(element_selection_condition(reloadable_element_name)),
            );
        }
        self
    }

    fn init_serializable_resource<R: ReplacableType + Default + Resource>(&mut self) -> &mut Self {
        let name = R::get_type_name();
        if !self.resources.contains(name) {
            self.resources.insert(name.to_string());
            trace!("adding resource {name}");
            let reloadable_element_name = self.name;
            self.add_systems(
                SerializeReloadables,
                serialize_replacable_resource::<R>
                    .run_if(element_selection_condition(reloadable_element_name)),
            )
            .add_systems(
                DeserializeReloadables,
                deserialize_replacable_resource_with_default::<R>
                    .run_if(element_selection_condition(reloadable_element_name)),
            );
        }
        self
    }

    fn insert_serializable_resource<R: ReplacableType + Resource>(
        &mut self,
        value: R,
    ) -> &mut Self {
        let name = R::get_type_name();
        if !self.resources.contains(name) {
            self.resources.insert(name.to_string());
            trace!("adding resource {name}");
            let reloadable_element_name = self.name;
            self.add_systems(
                SerializeReloadables,
                serialize_replacable_resource::<R>
                    .run_if(element_selection_condition(reloadable_element_name)),
            )
            .add_systems(
                DeserializeReloadables,
                deserialize_replacable_resource_with_value::<R>(value)
                    .run_if(element_selection_condition(reloadable_element_name)),
            );
        }
        self
    }

    fn register_serializable_component<C: ReplacableType + Component>(&mut self) -> &mut Self {
        let name = C::get_type_name();
        if !self.components.contains(name) {
            self.components.insert(name.to_string());
            let reloadable_element_name = self.name;
            self.add_systems(
                SerializeReloadables,
                serialize_replacable_component::<C>
                    .run_if(element_selection_condition(reloadable_element_name)),
            )
            .add_systems(
                DeserializeReloadables,
                deserialize_replacable_component::<C>
                    .run_if(element_selection_condition(reloadable_element_name)),
            );
        }
        self
    }

    fn reset_resource<R: Resource + Default>(&mut self) -> &mut Self {
        debug!("resetting resource");
        let name = self.name;
        self.add_systems(
            OnReloadComplete,
            (move |mut commands: Commands| {
                commands.insert_resource(R::default());
            })
            .run_if(element_selection_condition(name)),
        );
        self
    }

    fn reset_resource_to_value<R: Resource>(&mut self, value: R) -> &mut Self {
        debug!("resetting resource");
        let name = self.name;
        let mut container = Some(value);
        self.add_systems(
            OnReloadComplete,
            (move |mut commands: Commands| {
                if let Some(value) = container.take() {
                    commands.insert_resource(value);
                }
            })
            .run_if(element_selection_condition(name)),
        );
        self
    }

    fn clear_marked_on_reload<C: Component>(&mut self) -> &mut Self {
        let name = self.name;
        self.add_systems(
            CleanupReloaded,
            clear_marked_system::<C>.run_if(element_selection_condition(name)),
        );
        self
    }

    fn reset_setup<C: Component, M>(&mut self, systems: impl IntoSystemConfigs<M>) -> &mut Self {
        let name = self.name;
        self.add_systems(
            CleanupReloaded,
            clear_marked_system::<C>.run_if(element_selection_condition(name)),
        )
        .add_systems(
            OnReloadComplete,
            systems.run_if(element_selection_condition(name)),
        )
    }

    fn reset_setup_in_state<C: Component, S: States, M>(
        &mut self,
        state: S,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        let name = self.name;
        self.add_systems(
            CleanupReloaded,
            clear_marked_system::<C>.run_if(element_selection_condition(name)),
        )
        .add_systems(OnExit(state.clone()), clear_marked_system::<C>)
        .add_systems(
            PreUpdate,
            systems.run_if(
                in_state(state).and_then(
                    dexterous_developer_occured
                        .and_then(element_selection_condition(name))
                        .or_else(|res: Res<State<S>>| resource_changed::<State<S>>(res)),
                ),
            ),
        )
    }

    fn add_event<T: Event>(&mut self) -> &mut Self {
        let name = self.name;
        self.add_systems(
            OnReloadComplete,
            (move |world: &mut World| {
                EventRegistry::register_event::<T>(world);
            })
            .run_if(element_selection_condition(name)),
        );
        self
    }

    fn insert_state<S: FreelyMutableState + ReplacableType>(&mut self, state: S) -> &mut Self {
        let name = S::get_type_name();
        if !self.resources.contains(name) {
            {
                let state = state.clone();
                self.insert_serializable_resource(State::new(state.clone()))
                    .reset_resource::<NextState<S>>()
                    .add_event::<StateTransitionEvent<S>>()
                    .run_only_on_first_load(name, move |world: &mut World| {
                        world.send_event(StateTransitionEvent {
                            exited: None,
                            entered: Some(state.clone()),
                        });
                    });
            }

            let schedules = &mut self.schedules;

            let wrapped: WrappedSchedule = WrappedSchedule::new(StateTransition);

            if let Some((schedule, _)) = schedules.get_mut(&wrapped) {
                trace!("Adding systems to schedule");
                S::register_state(schedule);
            } else {
                trace!("Creating schedule with systems");
                let reloadable = ReloadableSchedule::new(wrapped.clone());
                let mut schedule = Schedule::new(reloadable.clone());

                S::register_state(&mut schedule);

                schedules.insert(wrapped, (schedule, reloadable));
            }
        }

        self
    }

    fn add_sub_state<S: SubStates + ReplacableType>(&mut self) -> &mut Self {
        let name = S::get_type_name();
        if !self.resources.contains(name) {
            self.register_serializable_resource::<State<S>>()
                .reset_resource::<NextState<S>>()
                .add_event::<StateTransitionEvent<S>>();

            let schedules = &mut self.schedules;

            let wrapped: WrappedSchedule = WrappedSchedule::new(StateTransition);

            if let Some((schedule, _)) = schedules.get_mut(&wrapped) {
                trace!("Adding systems to schedule");
                S::register_sub_state_systems(schedule);
            } else {
                trace!("Creating schedule with systems");
                let reloadable = ReloadableSchedule::new(wrapped.clone());
                let mut schedule = Schedule::new(reloadable.clone());

                S::register_sub_state_systems(&mut schedule);

                schedules.insert(wrapped, (schedule, reloadable));
            }
        }

        self
    }

    fn add_computed_state<S: ComputedStates + ReplacableType>(&mut self) -> &mut Self {
        let name = S::get_type_name();
        if !self.resources.contains(name) {
            self.register_serializable_resource::<State<S>>()
                .add_event::<StateTransitionEvent<S>>();

            let schedules = &mut self.schedules;

            let wrapped: WrappedSchedule = WrappedSchedule::new(StateTransition);

            if let Some((schedule, _)) = schedules.get_mut(&wrapped) {
                trace!("Adding systems to schedule");
                S::register_computed_state_systems(schedule);
            } else {
                trace!("Creating schedule with systems");
                let reloadable = ReloadableSchedule::new(wrapped.clone());
                let mut schedule = Schedule::new(reloadable.clone());

                S::register_computed_state_systems(&mut schedule);

                schedules.insert(wrapped, (schedule, reloadable));
            }
        }

        self
    }

    fn enable_state_scoped_entities<S: States + ReplacableType>(&mut self) -> &mut Self {
        self.register_serializable_component::<StateScoped<S>>()
            .add_systems(
                StateTransition,
                clear_state_scoped_entities::<S>
                    .after(ExitSchedules::<S>::default())
                    .before(TransitionSchedules::<S>::default()),
            )
    }
}

fn element_selection_condition(name: &'static str) -> impl Fn(Option<Res<ReloadSettings>>) -> bool {
    move |settings| {
        if let Some(settings) = settings {
            if let Some(current) = settings.reloadable_element_selection {
                if current != name {
                    return false;
                }
            }
        }
        true
    }
}
