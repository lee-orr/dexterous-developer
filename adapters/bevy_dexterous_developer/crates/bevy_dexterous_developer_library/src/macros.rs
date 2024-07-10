pub use paste::paste;

#[cfg(feature = "hot")]
mod hot {
    #[macro_export]
    macro_rules! reloadable_main {
        ($f:ident ($attr: ident) $body:block) => {
            fn reloadable_main_implementation($attr: impl bevy_dexterous_developer::InitialPlugins) $body

            #[no_mangle]
            pub extern "Rust" fn dexterous_developer_instance_main(_: &mut ()) {
                println!("Setting Up With Hot Reload Plugin");
                reloadable_main_implementation(bevy_dexterous_developer::HotReloadPlugin);
            }

            pub fn $f() {
                println!("Setting Up Without Hot Reload Plugin");
                reloadable_main_implementation(bevy_dexterous_developer::InitialPluginsEmpty);
            }
        };
    }
}

#[cfg(not(feature = "hot"))]
mod cold {
    #[macro_export]
    macro_rules! reloadable_main {
        ($f:ident ($attr: ident) $body:block) => {
            fn reloadable_main_implementation($attr: impl bevy_dexterous_developer::InitialPlugins) $body

            pub fn $f() {
                reloadable_main_implementation(bevy_dexterous_developer::InitialPluginsEmpty);
            }
        };
    }
}

#[macro_export]
macro_rules! reloadable_scope {
    () => {};
    ($f:ident ($attr:ident) $body:block) => {
        bevy_dexterous_developer::macros::paste!(reloadable_scope!(@inner $f, $attr, [<$f _dexterous_developered_inner_>], $body););
    };
    ($label: expr, $f:ident ($attr:ident) $body:block) => {
        bevy_dexterous_developer::macros::paste!(reloadable_scope!(@inner $f, $attr, [<$f _dexterous_developered_inner_ $label>], $body););
    };

    (@inner $f:ident, $attr:ident, $internal_f:tt, $body:block) => {
        #[no_mangle]
        pub fn $internal_f($attr: &mut ReloadableAppContents) $body

        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug)]
        pub struct $f;

        impl bevy_dexterous_developer::ReloadableSetup for $f {
            fn setup_function_name() -> &'static str {
                bevy::prelude::trace!("Reloadable Scope Name: {}", stringify!($internal_f));
                stringify!($internal_f)
            }

            fn default_function(app: &mut ReloadableAppContents) {
                bevy::prelude::trace!("Running Reloadable Function: {}", stringify!($f));
                bevy_dexterous_developer::macros::paste! {
                    $internal_f(app);
                }
            }
        }
    };
}
