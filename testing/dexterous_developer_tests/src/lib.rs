mod utils;

use std::{
    env,
    path::{Path, PathBuf},
};

use crate::utils::*;

async fn can_run_cold() {
    let mut project = TestProject::new("simple_cli_test", "can_run_cold").unwrap();
    let mut process = project.run_cold().await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.next_line_contains("Ran Update").await;

    process.exit().await;
}

async fn can_run_hot() {
    let mut project = TestProject::new("simple_cli_test", "can_run_hot").unwrap();
    let mut process = project.run_hot_cli().await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.exit().await;
}

async fn can_run_hot_and_edit() {
    let mut project = TestProject::new("simple_cli_test", "can_run_hot_and_edit").unwrap();
    let mut process = project.run_hot_cli().await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./updated_file.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Got some new text!"]).await;

    process.exit().await;
}

async fn can_run_hot_and_edit_with_launcher() {
    let mut project = TestProject::new("no_cli_test", "no_cli").unwrap();
    let mut process = project.run_hot_launcher("lib_simple").await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    project
        .write_file(
            PathBuf::from("./simple/src/update.rs").as_path(),
            include_str!("./updated_file.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Got some new text!"]).await;

    process.exit().await;
}

async fn can_run_with_reloadables() {
    let mut project: TestProject =
        TestProject::new("reloadables_test", "can_run_with_reloadables").unwrap();
    let mut process = project.run_hot_cli().await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    println!("INSERT REPLACABLE RESOURCE");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./insert_replacable_resource.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Got: Resource Added"]).await;

    println!("RESET REPLACABLE RESOURCE");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./reset_replacable_resource.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Got: Resource Replaced"]).await;

    process
        .send("And Updated\n")
        .expect("Failed to send empty line");

    process
        .wait_for_lines(&["Updated: Resource Replaced And Updated"])
        .await;

    println!("UPDATE RESOURCE WITHOUT RESET");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./update_replaceable_resource.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process
        .wait_for_lines(&["Retained: Resource Replaced And Updated"])
        .await;

    println!("UPDATE SCHEMA RESOURCE");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./update_schema_resource.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process
        .wait_for_lines(&["Got: Resource Replaced - Added Field"])
        .await;

    println!("INSERT REPLACABLE COMPONENTS");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./insert_replacable_components.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("first\n").expect("Failed to send empty line");

    process.wait_for_lines(&["first"]).await;

    process.send("second\n").expect("Failed to send empty line");

    process.wait_for_lines(&["second"]).await;

    println!("UPDATE COMPONENT SCHEMA");

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./update_schema_component.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["first"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["second"]).await;

    process.exit().await;
}

async fn can_run_remote() {
    let mut project = TestProject::new("simple_cli_test", "can_run_remote_host").unwrap();
    let mut client = TestProject::new("remote_client", "can_run_remote_client").unwrap();

    let mut host_process = project.run_host_cli("1234").await.unwrap();

    host_process.wait_for_lines(&["Serving on 1234"]).await;

    let mut process = client.run_client_cli("1234").await.unwrap();

    process.wait_for_lines(&["Got Message"]).await;
    process.exit().await;

    host_process.is_ready().await;

    let mut process = client.run_client_cli("1234").await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    project
        .write_file(
            PathBuf::from("src/update.rs").as_path(),
            include_str!("./updated_file.txt"),
        )
        .expect("Couldn't update file");

    process.has_updated().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Got some new text!"]).await;

    process.exit().await;
    host_process.exit().await;
}

async fn can_update_assets() {
    let mut project = TestProject::new("simple_cli_test", "can_update_assets_host").unwrap();
    let mut client = TestProject::new("remote_client", "can_update_assets_client").unwrap();

    let mut host_process = project.run_host_cli("2345").await.unwrap();

    host_process.wait_for_lines(&["Serving on 2345"]).await;

    let mut process = client.run_client_cli("2345").await.unwrap();

    process.wait_for_lines(&["Got Message"]).await;
    process.exit().await;

    host_process.is_ready().await;

    let mut process = client.run_client_cli("2345").await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    project
        .write_file(
            PathBuf::from("assets/nesting/another_placeholder.txt").as_path(),
            "changed content",
        )
        .expect("Couldn't update file");

    process.wait_for_lines(&["Downloaded Asset"]).await;

    process.exit().await;
    host_process.exit().await;
}

async fn can_run_existing(path: &Path) {
    let mut project = TestProject::existing_project(path, "can_run_existing").unwrap();

    let mut process = project.run_existing().await.unwrap();

    process.is_ready().await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.send("\n").expect("Failed to send empty line");

    process.wait_for_lines(&["Ran Update"]).await;

    process.exit().await;
}

pub async fn run_tests() {
    let mut args = env::args();
    args.next();
    let argument = args.next().unwrap_or_default();

    match argument.as_str() {
        "cold" => {
            println!("Can run cold");
            can_run_cold().await;
        }
        "hot" => {
            println!("Can run hot cli");
            can_run_hot().await;
        }
        "edit" => {
            println!("Can edit with hot reload cli");
            can_run_hot_and_edit().await;
        }
        "launcher" => {
            println!("Can edit with hot reload launcher");
            can_run_hot_and_edit_with_launcher().await;
        }
        "reloadables" => {
            println!("Can handle reloadables");
            can_run_with_reloadables().await;
        }
        "remote" => {
            println!("Can run remote");
            can_run_remote().await;
        }
        "asset" => {
            println!("Can update asset");
            can_update_assets().await;
        }
        "existing" => {
            println!("Can run existing assets");
            let libs = args.next().expect("No next lib set");
            let mut libs = PathBuf::from(libs);
            if !libs.is_absolute() {
                libs = std::env::current_dir().unwrap().join(libs);
            }
            if !libs.exists() || !libs.is_dir() {
                panic!("libs should be a directory");
            }
            let libs = libs.canonicalize().unwrap();
            can_run_existing(&libs).await;
        }
        _ => {
            eprintln!("{argument} is an invalid test");
            println!("Valid tests are:");
            println!("cold");
            println!("hot");
            println!("edit");
            println!("launcher");
            println!("reloadables");
            println!("remote");
            println!("asset");
            std::process::exit(1)
        }
    }
}

#[cfg(test)]
mod test {
    // #[tokio::test]
    // async fn can_run_cold() {
    //     super::can_run_cold().await;
    // }
    // #[tokio::test]
    // async fn can_run_hot() {
    //     super::can_run_hot().await;
    // }
    // #[tokio::test]
    // async fn can_run_hot_and_edit() {
    //     super::can_run_hot_and_edit().await;
    // }
    // #[tokio::test]
    // async fn can_run_hot_and_edit_with_launcher() {
    //     super::can_run_hot_and_edit_with_launcher().await;
    // }
    // #[tokio::test]
    // async fn can_run_with_reloadables() {
    //     super::can_run_with_reloadables().await;
    // }
    #[tokio::test]
    async fn can_run_remote() {
        super::can_run_remote().await;
    }
    // #[tokio::test]
    // async fn can_update_assets() {
    //     super::can_update_assets().await;
    // }
}
