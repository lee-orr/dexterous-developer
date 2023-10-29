mod update;

use dexterous_developer::{bevy_support::*, bevy_app_setup};
use std::str::Utf8Error;
use thiserror::Error;

use bevy::{
    asset::{io::Reader, *},
    prelude::*,
    utils::BoxedFuture,
    MinimalPlugins,
};

fn terminal_runner(mut app: App) {
    app.update();
    for line in std::io::stdin().lines() {
        println!("Runner Got {line:?}");
        let typed: String = line.unwrap_or_default();
        if typed == "exit" {
            println!("Exiting");
            return;
        }
        println!("Running Update");
        app.update();
        println!("Update Ended");
    }
}

#[bevy_app_setup]
pub fn bevy_main(initial_plugins: InitializeApp) {
    initial_plugins
        .initialize::<MinimalPlugins>()
        .app_with_runner(terminal_runner)
        .add_plugins(AssetPlugin {
            mode: AssetMode::Unprocessed,
            watch_for_changes_override: Some(true),
            ..default()
        })
        .init_asset::<TextAsset>()
        .init_asset_loader::<TextAssetLoader>()
        .add_plugins(update::MyPlugin);
}

#[derive(Resource)]
pub struct Text(pub Handle<TextAsset>);

#[derive(Asset, TypePath, Debug)]
pub struct TextAsset {
    pub value: String,
}

#[derive(Error, Debug)]
pub enum TextAssetError {
    #[error("IO Failed")]
    IO(#[from] std::io::Error),
    #[error("Couldn't process UTF8")]
    Utf8(#[from] Utf8Error),
}

#[derive(Default)]
pub struct TextAssetLoader;

impl AssetLoader for TextAssetLoader {
    type Asset = TextAsset;
    type Settings = ();
    type Error = TextAssetError;
    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a (),
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            println!("Loading file into memory...");
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            println!("Got buffer");
            let text = std::str::from_utf8(&bytes)?;
            println!("Read text: {text}");
            Ok(TextAsset {
                value: text.to_string(),
            })
        })
    }

    fn extensions(&self) -> &[&str] {
        &["txt"]
    }
}
