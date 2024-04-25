pub mod library_holder;

use std::{
    ffi::c_void,
    sync::{atomic::AtomicU32, Arc},
};

use camino::{Utf8Path, Utf8PathBuf};
use crossbeam::{atomic::AtomicCell, channel::Sender};
use dexterous_developer_internal::hot::{CallResponse, HotReloadInfo, UpdatedAsset};
use dexterous_developer_types::{cargo_path_utils::dylib_path, HotReloadMessage, Target};
use futures_util::StreamExt;
use once_cell::sync::OnceCell;
use safer_ffi::prelude::{c_slice, ffi_export};
use thiserror::Error;
use tokio_tungstenite::connect_async;
use tracing::info;
use url::Url;

use crate::library_holder::LibraryHolder;

pub fn run_reloadable_app(
    working_directory: &Utf8Path,
    library_path: &Utf8Path,
    server: url::Url,
) -> Result<(), DylibRunnerError> {
    if !library_path.exists() {
        return Err(DylibRunnerError::LibraryDirectoryDoesntExist(
            library_path.to_owned(),
        ));
    }
    if !working_directory.exists() {
        return Err(DylibRunnerError::WorkingDirectoryDoesntExist(
            working_directory.to_owned(),
        ));
    }

    let dylib_paths = dylib_path();
    if !dylib_paths.contains(&library_path.to_owned()) {
        return Err(DylibRunnerError::DylibPathsMissingLibraries);
    }

    let current_target = Target::current().ok_or(DylibRunnerError::NoCurrentTarget)?;

    let address = server.join("target/")?;
    info!("Setting Up Route {address}");
    let mut address = address.join(current_target.as_str())?;
    let initial_scheme = address.scheme();
    let new_scheme = match initial_scheme {
        "http" => "ws",
        "https" => "wss",
        "ws" => "ws",
        "wss" => "wss",
        scheme => {
            return Err(DylibRunnerError::InvalidScheme(
                server.clone(),
                scheme.to_string(),
            ))
        }
    };

    address
        .set_scheme(new_scheme)
        .map_err(|_e| DylibRunnerError::InvalidScheme(server.clone(), "Unknown".to_string()))?;

    let (tx, rx) = crossbeam::channel::unbounded::<DylibRunnerMessage>();

    let handle = {
        let server = server.clone();
        let address = address.clone();
        let _library_path = library_path;
        let _working_directory = working_directory;

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on({
                    let result = remote_connection(address, server, tx.clone());
                    let _ = tx.send(DylibRunnerMessage::ConnectionClosed);
                    result
                })
        })
    };

    let (initial, id) = {
        let mut library = None;
        let mut id = None;
        loop {
            if library.is_some() {
                break;
            }
            let initial = rx.recv()?;
            match initial {
                DylibRunnerMessage::ConnectionClosed => {
                    let _ = handle.join().map_err(DylibRunnerError::JoinHandleFailed)?;
                    return Ok(());
                }
                DylibRunnerMessage::LoadRootLib {
                    build_id,
                    local_path,
                } => {
                    library = Some(LibraryHolder::new(&local_path)?);
                    id = Some(build_id);
                }
                DylibRunnerMessage::AssetUpdated { .. } => {
                    continue;
                }
            }
        }
        (
            library.ok_or(DylibRunnerError::NoInitialLibrary)?,
            id.ok_or(DylibRunnerError::NoInitialLibrary)?,
        )
    };
    let initial = Arc::new(initial);

    LAST_UPDATE_VERSION.store(id, std::sync::atomic::Ordering::SeqCst);
    ORIGINAL_LIBRARY
        .set(initial.clone())
        .map_err(|_| DylibRunnerError::OnceCellError)?;

    let mut info: HotReloadInfo = HotReloadInfo {
        internal_last_update_version: last_update_version,
        internal_update_ready: update_ready,
        internal_call_on_current: call,
        internal_set_update_callback: update_callback,
        internal_update: update,
        internal_set_asset_update_callback: update_asset_callback,
    };

    let _handle = std::thread::spawn(|| update_loop(rx, handle));

    initial.call::<HotReloadInfo>("dexterous_developer_internal_main", &mut info)?;

    Ok(())
}

async fn remote_connection(
    address: Url,
    _server: Url,
    _tx: Sender<DylibRunnerMessage>,
) -> Result<(), DylibRunnerError> {
    info!("Connecting To {address}");

    let (ws_stream, _) = connect_async(address).await?;

    let (_, mut read) = ws_stream.split();

    loop {
        let Some(msg) = read.next().await else {
            return Ok(());
        };

        let msg = msg?;

        match msg {
            tokio_tungstenite::tungstenite::Message::Binary(binary) => {
                let msg: HotReloadMessage = rmp_serde::from_slice(&binary)?;
                info!("Received Hot Reload Message: {msg:?}");
            }
            _ => {
                return Ok(());
            }
        }
    }
}

fn update_loop(
    rx: crossbeam::channel::Receiver<DylibRunnerMessage>,
    handle: std::thread::JoinHandle<Result<(), DylibRunnerError>>,
) -> Result<(), DylibRunnerError> {
    loop {
        let message = rx.recv()?;
        match message {
            DylibRunnerMessage::ConnectionClosed => {
                let _ = handle.join().map_err(DylibRunnerError::JoinHandleFailed)?;
                eprintln!("Connection Closed");
                return Ok(());
            }
            DylibRunnerMessage::LoadRootLib {
                build_id,
                local_path,
            } => {
                let library = LibraryHolder::new(&local_path)?;
                NEXT_UPDATE_VERSION.store(build_id, std::sync::atomic::Ordering::SeqCst);
                NEXT_LIBRARY.store(Some(Arc::new(library)));
                unsafe {
                    if let Some(Some(callback)) = UPDATED_CALLBACK.as_ptr().as_ref() {
                        callback(build_id);
                    }
                }
            }
            DylibRunnerMessage::AssetUpdated { local_path, name } => unsafe {
                if let Some(Some(callback)) = UPDATED_ASSET_CALLBACK.as_ptr().as_ref() {
                    let inner_local_path = c_slice::Box::from(
                        local_path
                            .to_string()
                            .as_bytes()
                            .iter()
                            .copied()
                            .collect::<Box<[u8]>>(),
                    );
                    let inner_name =
                        c_slice::Box::from(name.as_bytes().iter().copied().collect::<Box<[u8]>>());
                    callback(UpdatedAsset {
                        inner_name,
                        inner_local_path,
                    });
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum DylibRunnerMessage {
    ConnectionClosed,
    LoadRootLib {
        build_id: u32,
        local_path: Utf8PathBuf,
    },
    AssetUpdated {
        local_path: Utf8PathBuf,
        name: String,
    },
}

#[derive(Error, Debug)]
pub enum DylibRunnerError {
    #[error("Dylib Runner IO Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("Dynamic Library Paths don't include current library path")]
    DylibPathsMissingLibraries,
    #[error("Couldn't determine current Target")]
    NoCurrentTarget,
    #[error("Couldn't parse URL {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Couldn't set websocket scheme for {0:?} - {1} is an invalid scheme")]
    InvalidScheme(url::Url, String),
    #[error("Working Directory does not exist - {0:?}")]
    WorkingDirectoryDoesntExist(Utf8PathBuf),
    #[error("Library Directory does not exist - {0:?}")]
    LibraryDirectoryDoesntExist(Utf8PathBuf),
    #[error("WebSocket Error {0}")]
    WebSocketError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("RMP Parse Error {0}")]
    RmpParseError(#[from] rmp_serde::decode::Error),
    #[error("Crossbeam Channel Failed {0}")]
    CrosbeamChannelError(#[from] crossbeam::channel::RecvError),
    #[error("Join Handle Failed")]
    JoinHandleFailed(std::boxed::Box<(dyn std::any::Any + std::marker::Send + 'static)>),
    #[error("Library Holder Error {0}")]
    LibraryError(#[from] library_holder::LibraryError),
    #[error("Couldn't Open Initial Library")]
    NoInitialLibrary,
    #[error("Original Library Already Set")]
    OnceCellError,
}

pub static LAST_UPDATE_VERSION: AtomicU32 = AtomicU32::new(0);
pub static NEXT_UPDATE_VERSION: AtomicU32 = AtomicU32::new(0);
pub static ORIGINAL_LIBRARY: OnceCell<Arc<LibraryHolder>> = OnceCell::new();
pub static LAST_LIBRARY: AtomicCell<Option<Arc<LibraryHolder>>> = AtomicCell::new(None);
pub static CURRENT_LIBRARY: AtomicCell<Option<Arc<LibraryHolder>>> = AtomicCell::new(None);
pub static NEXT_LIBRARY: AtomicCell<Option<Arc<LibraryHolder>>> = AtomicCell::new(None);
pub static UPDATED_CALLBACK: AtomicCell<Option<extern "C" fn(u32) -> ()>> = AtomicCell::new(None);
pub static UPDATED_ASSET_CALLBACK: AtomicCell<Option<extern "C" fn(UpdatedAsset) -> ()>> =
    AtomicCell::new(None);

#[ffi_export]
fn last_update_version() -> u32 {
    LAST_UPDATE_VERSION.load(std::sync::atomic::Ordering::SeqCst)
}

#[ffi_export]
fn update_ready() -> bool {
    let last = LAST_UPDATE_VERSION.load(std::sync::atomic::Ordering::SeqCst);
    let next = NEXT_UPDATE_VERSION.load(std::sync::atomic::Ordering::SeqCst);
    next > last
}

#[ffi_export]
fn update() -> bool {
    let next = NEXT_UPDATE_VERSION.load(std::sync::atomic::Ordering::SeqCst);
    let old = LAST_UPDATE_VERSION.fetch_max(next, std::sync::atomic::Ordering::SeqCst);
    if old < next {
        LAST_LIBRARY.store(CURRENT_LIBRARY.swap(NEXT_LIBRARY.take()));
        true
    } else {
        false
    }
}

#[ffi_export]
fn update_callback(callback: extern "C" fn(u32) -> ()) {
    UPDATED_CALLBACK.store(Some(callback))
}

#[ffi_export]
fn update_asset_callback(callback: extern "C" fn(UpdatedAsset) -> ()) {
    UPDATED_ASSET_CALLBACK.store(Some(callback))
}

#[ffi_export]
fn call(name: c_slice::Ref<'_, u8>, mut args: *mut c_void) -> CallResponse {
    unsafe {
        let name = std::str::from_utf8(name.as_slice());
        let name = match name {
            Ok(name) => name,
            Err(e) => {
                return CallResponse {
                    success: false,
                    error: c_slice::Box::from(
                        format!("Couldn't Parse Function Name: {e}")
                            .as_bytes()
                            .iter()
                            .copied()
                            .collect::<Box<[u8]>>(),
                    ),
                };
            }
        };
        let Some(current) = CURRENT_LIBRARY.as_ptr().as_ref() else {
            return CallResponse {
                success: false,
                error: c_slice::Box::from(
                    "Failed to get current library"
                        .to_string()
                        .as_bytes()
                        .iter()
                        .copied()
                        .collect::<Box<[u8]>>(),
                ),
            };
        };
        let Some(current) = current.as_ref().cloned() else {
            return CallResponse {
                success: false,
                error: c_slice::Box::from(
                    "Current Library Not Set"
                        .to_string()
                        .as_bytes()
                        .iter()
                        .copied()
                        .collect::<Box<[u8]>>(),
                ),
            };
        };
        let result = current.call(name, &mut args);
        match result {
            Ok(_) => CallResponse {
                success: true,
                error: c_slice::Box::default(),
            },
            Err(e) => CallResponse {
                success: false,
                error: c_slice::Box::from(
                    format!("Error Running Function: {e}")
                        .as_bytes()
                        .iter()
                        .copied()
                        .collect::<Box<[u8]>>(),
                ),
            },
        }
    }
}
