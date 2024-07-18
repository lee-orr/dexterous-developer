use cargo_metadata::Metadata;
use debounced::debounced;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU32},
        Arc,
    },
    time::Duration,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use anyhow::bail;

use camino::{Utf8Path, Utf8PathBuf};
use dexterous_developer_types::{cargo_path_utils::dylib_path, Target, TargetBuildSettings};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::Mutex,
    task::JoinHandle,
};
use tracing::{debug, error, info, trace};

use crate::{
    types::{
        BuildOutputMessages, Builder, BuilderIncomingMessages, BuilderOutgoingMessages,
        HashedFileRecord,
    },
    zig_downloader::zig_path,
};

pub struct IncrementalBuilder {
    target: Target,
    settings: TargetBuildSettings,
    incoming: tokio::sync::mpsc::UnboundedSender<BuilderIncomingMessages>,
    outgoing: tokio::sync::broadcast::Sender<BuilderOutgoingMessages>,
    output: tokio::sync::broadcast::Sender<BuildOutputMessages>,
    #[allow(dead_code)]
    handle: tokio::task::JoinHandle<()>,
}

async fn build(
    target: Target,
    TargetBuildSettings {
        working_dir,
        package_or_example,
        features,
        mut manifest_path,
        additional_library_directories,
        apple_sdk_directory,
        ..
    }: TargetBuildSettings,
    previous_versions: Arc<Mutex<Vec<(String, Utf8PathBuf)>>>,
    sender: tokio::sync::broadcast::Sender<BuildOutputMessages>,
    id: u32,
) -> Result<(), anyhow::Error> {
    info!("Incremental Build {id} Started");
    let zig = zig_path().await?;
    let linker = which::which("dexterous_developer_incremental_linker")?;
    let Ok(linker) = Utf8PathBuf::from_path_buf(linker) else {
        bail!("Couldn't get linker path");
    };
    let linker = linker.canonicalize_utf8()?;
    let cc = which::which("dexterous_developer_incremental_c_compiler")?;
    let Ok(cc) = Utf8PathBuf::from_path_buf(cc) else {
        bail!("Couldn't get cc path");
    };
    let cc = cc.canonicalize_utf8()?;

    let (artifact_name, artifact_file_name) = {
        let mut cmd = Command::new("cargo");
        cmd.arg("metadata");
        if let Some(manifest_path) = &manifest_path {
            cmd.arg("--manifest-path").arg(manifest_path);
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            bail!("Failed to get Cargo metadata");
        }
        let output: Metadata = serde_json::from_slice(&output.stdout)?;

        match &package_or_example {
            dexterous_developer_types::PackageOrExample::DefaulPackage => {
                let Some(root) = (if let Some(package) = output.root_package() {
                    find_package_target(package, target, id)
                } else if output.workspace_default_members.len() == 1 {
                    let default_member = output.workspace_default_members.first().unwrap();
                    if let Some(package) = output.packages.iter().find(|p| p.id == *default_member)
                    {
                        find_package_target(package, target, id)
                    } else {
                        None
                    }
                } else {
                    None
                }) else {
                    bail!("Can't find default package target");
                };
                root
            }
            dexterous_developer_types::PackageOrExample::Package(package) => {
                let Some(package) = output.packages.iter().find(|p| p.name == *package) else {
                    bail!("Couldn't find package");
                };
                let Some(p) = find_package_target(package, target, id) else {
                    bail!("Can't find package target");
                };
                p
            }
            dexterous_developer_types::PackageOrExample::Example(e) => {
                let Some((example_target, package)) = output
                    .packages
                    .into_iter()
                    .flat_map(|e| {
                        e.targets
                            .iter()
                            .map(|t| (t.clone(), e.clone()))
                            .collect::<Vec<_>>()
                    })
                    .find(|(t, _)| t.is_example() && t.name == *e)
                else {
                    bail!("No such example");
                };

                if manifest_path.is_none() {
                    manifest_path = Some(package.manifest_path);
                }
                let artifact_name = example_target.name.clone();
                let artifact_file_name = target.dynamic_lib_name(&format!("{artifact_name}.{id}"));
                (artifact_name, artifact_file_name)
            }
        }
    };

    let incremental_run_settings = if id == 1 {
        IncrementalRunParams::InitialRun
    } else {
        let target_dir = Utf8PathBuf::from(format!("./target/hot-reload/{target}/{target}/debug"))
            .canonicalize_utf8()?;
        let deps = target_dir.join("deps");
        let examples = target_dir.join("examples");
        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir)?;
        }
        if !deps.exists() {
            std::fs::create_dir_all(&deps)?;
        }
        if !examples.exists() {
            std::fs::create_dir_all(&examples)?;
        }
        IncrementalRunParams::Patch {
            id,
            timestamp: std::time::SystemTime::now(),
            previous_versions: {
                let previous_versions = previous_versions.lock().await;
                previous_versions
                    .iter()
                    .filter_map(|(name, path)| {
                        if path.exists() {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            },
        }
    };

    let target_dir =
        Utf8PathBuf::from(format!("./target/hot-reload/{target}")).canonicalize_utf8()?;
    let default_out = target_dir.join(format!("{target}")).join("debug");
    let deps = default_out.join("deps");
    let examples = default_out.join("examples");
    let artifact_path = default_out.join(&artifact_file_name);

    let mut cargo = Command::new("cargo");
    if let Some(working_dir) = working_dir {
        cargo.current_dir(&working_dir);
    }

    let mut lib_directories = additional_library_directories.clone();
    lib_directories.push(default_out.clone());
    lib_directories.push(deps.clone());
    lib_directories.push(examples.clone());

    for dir in &apple_sdk_directory {
        lib_directories.push(dir.join("usr").join("lib"));
    }

    cargo
        .env_remove("LD_DEBUG")
        .env("ZIG_PATH", &zig)
        .env("CC", cc)
        .env(
            "DEXTEROUS_DEVELOPER_LINKER_TARGET",
            target.zig_linker_target(),
        )
        .env("DEXTEROUS_DEVELOPER_PACKAGE_NAME", &artifact_name)
        .env("DEXTEROUS_DEVELOPER_OUTPUT_FILE", &artifact_path)
        .env(
            "DEXTEROUS_DEVELOPER_LIB_DIRECTORES",
            serde_json::to_string(&lib_directories)?,
        )
        .env(
            "DEXTEROUS_DEVELOPER_FRAMEWORK_DIRECTORES",
            serde_json::to_string(
                &apple_sdk_directory
                    .iter()
                    .map(|v| v.join("System/Library/Frameworks"))
                    .collect::<Vec<_>>(),
            )?,
        )
        .env(
            "DEXTEROUS_DEVELOPER_INCREMENTAL_RUN",
            serde_json::to_string(&incremental_run_settings)?,
        )
        .env("RUSTFLAGS", "-Cprefer-dynamic")
        .env("CARGO_TARGET_DIR", target_dir)
        .arg("rustc");

    if let Some(manifest) = &manifest_path {
        cargo.arg("--manifest-path").arg(manifest.canonicalize()?);
    }

    match &package_or_example {
        dexterous_developer_types::PackageOrExample::DefaulPackage => {}
        dexterous_developer_types::PackageOrExample::Package(package) => {
            cargo.arg("--lib").arg("-p").arg(package.as_str());
        }
        dexterous_developer_types::PackageOrExample::Example(example) => {
            cargo.arg("--example").arg(example.as_str());
        }
    }

    if !features.is_empty() {
        cargo.arg("--features");
        cargo.arg(features.join(",").as_str());
    }

    cargo
        .arg("--message-format=json-render-diagnostics")
        .arg("--profile")
        .arg("dev")
        .arg("--target")
        .arg(target.to_string())
        .arg("--")
        .arg("-C")
        .arg(format!("linker={linker}"));

    let _ = sender.send(BuildOutputMessages::StartedBuild(id));

    let mut child = cargo
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut succeeded = false;

    let mut artifacts = Vec::with_capacity(20);

    let Some(output) = child.stdout.take() else {
        bail!("No Std Out");
    };

    let Some(error) = child.stderr.take() else {
        bail!("No Std Err");
    };

    tokio::spawn(async move {
        let mut out_reader = BufReader::new(error).lines();
        while let Ok(Some(line)) = out_reader.next_line().await {
            println!("Compilation - {line}");
        }
    });

    let mut out_reader = BufReader::new(output).lines();

    while let Some(line) = out_reader.next_line().await? {
        trace!("Compiler Output: {line}");
        let message = serde_json::from_str(&line)?;

        match &message {
            cargo_metadata::Message::CompilerArtifact(artifact) => {
                artifacts.push(artifact.clone());
            }
            cargo_metadata::Message::BuildFinished(finished) => {
                info!("Build Finished: {finished:?}");
                succeeded = finished.success;
            }
            msg => trace!("Compiler: {msg:?}"),
        }
    }

    if !succeeded {
        error!("Build Failed");
        bail!("Failed to build");
    }

    let mut libraries = HashMap::<String, Utf8PathBuf>::with_capacity(20);
    libraries.insert(artifact_file_name.clone(), artifact_path.clone());

    let initial_libraries = libraries
        .iter()
        .map(|(name, path)| (name.clone(), path.clone()))
        .collect::<Vec<_>>();

    let mut path_var = match env::var_os("PATH") {
        Some(var) => env::split_paths(&var)
            .filter_map(|p| Utf8PathBuf::try_from(p).ok())
            .collect(),
        None => Vec::new(),
    };

    let mut dylib_paths = dylib_path();
    let mut root_dirs = vec![default_out, deps, examples];

    path_var.append(&mut dylib_paths);
    path_var.append(&mut root_dirs);
    path_var.push(
        Utf8PathBuf::from_path_buf(env::current_dir()?)
            .unwrap_or_default()
            .join("target")
            .join("hot-reload")
            .join(target.to_string())
            .join(target.to_string())
            .join("debug")
            .join("deps"),
    );

    {
        let rustup_home = home::rustup_home()?;
        let toolchains = rustup_home.join("toolchains");
        let mut dir = tokio::fs::read_dir(toolchains).await?;

        while let Ok(Some(child)) = dir.next_entry().await {
            if child.file_type().await?.is_dir() {
                let path = Utf8PathBuf::from_path_buf(child.path()).unwrap_or_default();
                path_var.push(path.join("lib"));
            }
        }
    }

    trace!("Path Var for DyLib Search: {path_var:?}");

    let dir_collections = path_var.iter().map(|dir| {
        let dir = dir.clone();
        tokio::spawn(async {
            let Ok(mut dir) = tokio::fs::read_dir(dir).await else {
                return vec![];
            };
            let mut files = vec![];

            while let Ok(Some(child)) = dir.next_entry().await {
                let Ok(file_type) = child.file_type().await else {
                    continue;
                };

                if file_type.is_file() {
                    let Ok(path) = Utf8PathBuf::from_path_buf(child.path()) else {
                        continue;
                    };

                    if let Some(name) = path.file_name() {
                        files.push((name.to_owned(), path))
                    }
                }
            }

            files
        })
    });

    let searchable_files = join_all(dir_collections)
        .await
        .iter()
        .filter_map(|result| match result {
            Ok(v) => Some(v),
            Err(_) => None,
        })
        .flatten()
        .cloned()
        .collect::<HashMap<_, _>>();

    let mut dependencies = HashMap::new();

    for (name, library) in initial_libraries.iter() {
        process_dependencies_recursive(
            &searchable_files,
            &mut libraries,
            &mut dependencies,
            name,
            library,
        )?;
    }

    let libraries = {
        libraries
            .iter()
            .map(|(library, local_path)| {
                let file = std::fs::read(local_path)?;
                let hash = blake3::hash(&file);

                Ok(HashedFileRecord {
                    name: library.clone(),
                    local_path: local_path.clone(),
                    relative_path: Utf8PathBuf::from(format!("./{library}")),
                    hash: hash.as_bytes().to_owned(),
                    dependencies: dependencies
                        .get(library.as_str())
                        .cloned()
                        .unwrap_or_default(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    };

    {
        let mut previous = previous_versions.lock().await;
        previous.push((format!("{artifact_name}.{id}"), artifact_path.clone()));
    }

    let _ = sender.send(BuildOutputMessages::EndedBuild {
        id,
        libraries,
        root_library: artifact_file_name,
    });
    info!("Build {id} Completed");
    Ok(())
}

fn find_package_target(
    package: &cargo_metadata::Package,
    target: Target,
    id: u32,
) -> Option<(String, String)> {
    let targets = &package.targets;

    let package_target = if let Some(lib) = targets.iter().find(|target| target.is_lib()) {
        lib
    } else if let Some(default_run) = &package.default_run {
        targets
            .iter()
            .find(|target| target.is_bin() && &target.name == default_run)?
    } else if let Some(first_bin) = targets.iter().find(|target| target.is_bin()) {
        first_bin
    } else {
        return None;
    };

    let artifact_name = package_target.name.clone();
    let artifact_file_name = target.dynamic_lib_name(&format!("{artifact_name}.{id}"));

    Some((artifact_name, artifact_file_name))
}

fn process_dependencies_recursive(
    searchable_files: &HashMap<String, Utf8PathBuf>,
    libraries: &mut HashMap<String, Utf8PathBuf>,
    dependencies: &mut HashMap<String, Vec<String>>,
    current_library_name: &str,
    current_library: &Utf8Path,
) -> Result<(), anyhow::Error> {
    let file = fs::read(current_library)?;
    let file = goblin::Object::parse(&file)?;

    let dependency_vec = match file {
        goblin::Object::Elf(elf) => {
            let str_table = elf.dynstrtab;
            elf.dynamic
                .map(|dynamic| {
                    dynamic
                        .get_libraries(&str_table)
                        .iter()
                        .map(|v| v.to_string())
                        .collect()
                })
                .unwrap_or_default()
        }
        goblin::Object::PE(pe) => pe
            .libraries
            .iter()
            .map(|import| import.to_string())
            .collect(),
        goblin::Object::Mach(mach) => match mach {
            goblin::mach::Mach::Fat(fat) => {
                let mut vec = HashSet::new();
                while let Some(Ok(goblin::mach::SingleArch::MachO(arch))) = fat.into_iter().next() {
                    let imports = arch.imports()?;
                    let inner = imports.iter().map(|v| v.dylib.to_string());
                    vec.extend(inner);
                }
                vec
            }
            goblin::mach::Mach::Binary(std) => {
                std.imports()?.iter().map(|v| v.dylib.to_string()).collect()
            }
        },
        _ => HashSet::default(),
    };

    for library_name in dependency_vec.iter() {
        if library_name.is_empty() {
            continue;
        }
        if libraries.contains_key(library_name) {
            continue;
        }
        let Some(library_path) = searchable_files.get(library_name) else {
            debug!("Couldn't find library with name {library_name}");
            continue;
        };
        libraries.insert(library_name.to_string(), library_path.clone());
    }
    dependencies.insert(
        current_library_name.to_string(),
        dependency_vec.into_iter().collect(),
    );
    Ok(())
}

impl IncrementalBuilder {
    pub fn new(target: Target, settings: TargetBuildSettings) -> Self {
        let (incoming, incoming_rx) = tokio::sync::mpsc::unbounded_channel();
        let (outgoing_tx, _) = tokio::sync::broadcast::channel(100);
        let (output_tx, _) = tokio::sync::broadcast::channel(100);
        let id = Arc::new(AtomicU32::new(1));
        let build_active = Arc::new(AtomicBool::new(false));
        let build_pending = Arc::new(AtomicBool::new(false));
        let previous_versions = Arc::new(Mutex::new(vec![]));

        let handle = {
            let outgoing_tx = outgoing_tx.clone();
            let output_tx = output_tx.clone();
            let settings = settings.clone();
            let id = id.clone();
            tokio::spawn(async move {
                let mut should_build = false;

                let delay = Duration::from_secs(1);

                let stream = UnboundedReceiverStream::new(incoming_rx);

                let mut debounced = debounced(stream, delay);

                while let Some(recv) = debounced.next().await {
                    match recv {
                        BuilderIncomingMessages::RequestBuild => {
                            should_build = true;
                            trigger_build(
                                &build_active,
                                &build_pending,
                                &id,
                                &outgoing_tx,
                                target,
                                &settings,
                                &output_tx,
                                &previous_versions,
                            );
                        }
                        BuilderIncomingMessages::CodeChanged => {
                            trace!("Code Changed");
                            if should_build {
                                trigger_build(
                                    &build_active,
                                    &build_pending,
                                    &id,
                                    &outgoing_tx,
                                    target,
                                    &settings,
                                    &output_tx,
                                    &previous_versions,
                                );
                            }
                        }
                        BuilderIncomingMessages::AssetChanged(asset) => {
                            trace!("Builder Received Asset Change - {asset:?}");
                            let _ = output_tx.send(BuildOutputMessages::AssetUpdated(asset));
                        }
                    }
                }
            })
        };

        Self {
            settings,
            target,
            incoming,
            outgoing: outgoing_tx,
            output: output_tx,
            handle,
        }
    }
}

fn trigger_build(
    build_active: &Arc<AtomicBool>,
    build_pending: &Arc<AtomicBool>,
    id: &Arc<AtomicU32>,
    outgoing_tx: &tokio::sync::broadcast::Sender<BuilderOutgoingMessages>,
    target: Target,
    settings: &TargetBuildSettings,
    output_tx: &tokio::sync::broadcast::Sender<BuildOutputMessages>,
    previous_versions: &Arc<Mutex<Vec<(String, Utf8PathBuf)>>>,
) {
    trace!("Triggering Build");
    let previous = build_active.swap(true, std::sync::atomic::Ordering::SeqCst);
    if previous {
        build_pending.store(true, std::sync::atomic::Ordering::SeqCst);
    } else {
        let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = outgoing_tx.send(BuilderOutgoingMessages::BuildStarted);
        let output_tx = output_tx.clone();
        let settings = settings.clone();
        let build_pending = build_pending.clone();
        let build_active = build_active.clone();
        let previous_versions = previous_versions.clone();
        #[allow(clippy::let_underscore_future)]
        let _: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
            build(
                target,
                settings.clone(),
                previous_versions.clone(),
                output_tx.clone(),
                id,
            )
            .await?;

            loop {
                let pending = build_pending.swap(false, std::sync::atomic::Ordering::SeqCst);
                if pending {
                    build(
                        target,
                        settings.clone(),
                        previous_versions.clone(),
                        output_tx.clone(),
                        id,
                    )
                    .await?;
                } else {
                    break;
                }
            }
            build_active.swap(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
    }
}

impl Builder for IncrementalBuilder {
    fn target(&self) -> Target {
        self.target
    }

    fn incoming_channel(
        &self,
    ) -> tokio::sync::mpsc::UnboundedSender<crate::types::BuilderIncomingMessages> {
        self.incoming.clone()
    }

    fn outgoing_channel(
        &self,
    ) -> (
        tokio::sync::broadcast::Receiver<crate::types::BuilderOutgoingMessages>,
        tokio::sync::broadcast::Receiver<crate::types::BuildOutputMessages>,
    ) {
        (self.outgoing.subscribe(), self.output.subscribe())
    }

    fn root_lib_name(&self) -> Option<String> {
        None
    }

    fn get_code_subscriptions(&self) -> Vec<camino::Utf8PathBuf> {
        self.settings.code_watch_folders.clone()
    }

    fn get_asset_subscriptions(&self) -> Vec<camino::Utf8PathBuf> {
        self.settings.asset_folders.clone()
    }

    fn builder_type(&self) -> dexterous_developer_types::BuilderTypes {
        dexterous_developer_types::BuilderTypes::Incremental
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum IncrementalRunParams {
    InitialRun,
    Patch {
        id: u32,
        timestamp: std::time::SystemTime,
        previous_versions: Vec<String>,
    },
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;
    use dexterous_developer_types::PackageOrExample;
    use test_temp_dir::*;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;
    use tokio::time::timeout;

    #[tokio::test]
    async fn can_build_a_package() {
        let dir = test_temp_dir!();
        let dir_path = dir.as_path_untracked().to_path_buf();
        let cargo = dir_path.join("Cargo.toml");

        let _ = Command::new("cargo")
            .current_dir(&dir_path)
            .arg("init")
            .arg("--name=test_lib")
            .arg("--vcs=none")
            .arg("--lib")
            .output()
            .await
            .expect("Failed to create test project");

        {
            let mut file = tokio::fs::File::options()
                .append(true)
                .open(&cargo)
                .await
                .expect("Couldn't open cargo toml");
            file.write_all(
                r#"[lib]
            crate-type = ["rlib", "dylib"]"#
                    .as_bytes(),
            )
            .await
            .expect("Couldn't write to cargo toml");
            file.sync_all()
                .await
                .expect("Couldn't flush write to cargo toml");
        }

        let target = Target::current().expect("Couldn't determine current target");

        let build = IncrementalBuilder::new(
            target,
            TargetBuildSettings {
                package_or_example: PackageOrExample::Package("test_lib".to_string()),
                working_dir: Utf8PathBuf::from_path_buf(dir_path).ok(),
                code_watch_folders: vec![Utf8PathBuf::from_path_buf(
                    dir.as_path_untracked().join("src"),
                )
                .unwrap()],
                ..Default::default()
            },
        );

        let (mut builder_messages, mut build_messages) = build.outgoing_channel();

        build
            .incoming
            .send(BuilderIncomingMessages::RequestBuild)
            .expect("Failed to request build");

        let msg = timeout(Duration::from_secs(10), builder_messages.recv())
            .await
            .expect("Didn't recieve watcher message on time")
            .expect("Didn't recieve watcher message");

        assert!(matches!(msg, BuilderOutgoingMessages::BuildStarted));

        let mut started = false;
        let mut ended = false;
        let mut root_lib_confirmed = false;
        let mut library_update_received = false;

        let mut messages = Vec::new();

        if let Err(e) = timeout(Duration::from_secs(10), async {
            loop {
                let msg = build_messages
                    .recv()
                    .await
                    .expect("Couldn't get build message");
                messages.push(msg.clone());
                match msg {
                    BuildOutputMessages::StartedBuild(id) => {
                        if started {
                            panic!("Started more than once");
                        }
                        assert_eq!(id, 1);
                        started = true;
                    }
                    BuildOutputMessages::EndedBuild {
                        id,
                        libraries,
                        root_library,
                    } => {
                        assert_eq!(id, 1);
                        ended = true;
                        assert_eq!(root_library, target.dynamic_lib_name("test_lib"));
                        root_lib_confirmed = true;
                        for HashedFileRecord {
                            local_path,
                            dependencies,
                            name,
                            ..
                        } in libraries.into_iter()
                        {
                            assert!(local_path.exists());
                            if name == target.dynamic_lib_name("test_lib") {
                                assert!(dependencies.len() == 1, "Dependencies: {dependencies:?}");
                                library_update_received = true;
                            }
                        }
                        break;
                    }
                    BuildOutputMessages::AssetUpdated(_) => {}
                    BuildOutputMessages::KeepAlive => {}
                }
            }
        })
        .await
        {
            panic!("Failed - {e:?}\n{messages:?}");
        }

        assert!(started);
        assert!(ended);
        assert!(root_lib_confirmed);
        assert!(library_update_received);
    }
}
