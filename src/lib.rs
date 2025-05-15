use anyhow::Result;
use bollard::Docker;
use serde::Serialize;
use std::path::PathBuf;

mod generate_flake;
mod generate_lock;
mod execute_command;
mod execute_build;
mod container_utils;

use generate_flake::generate_flake_file;
use generate_lock::generate_flake_lock;
use execute_build::execute_nix_build;
use container_utils::{setup_container, cleanup_container};
use execute_command::execute_command;

pub const FLAKE_TEMPLATE: &'static str = include_str!("../templates/flake.nix.hbs");

// ANSI color codes for terminal output
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const CYAN: &str = "\x1b[36m";
pub const MAGENTA: &str = "\x1b[35m";

/// Represents an extra Nix input to include
#[derive(Serialize)]
pub struct ExtraInput {
    pub name: String,
    pub url: String,
}

/// Build a Rust project with Nix inside Docker
pub async fn build_with_nix(
    nix_image: &str,
    project_path: &str,
    targets: &[&str],
    extra_inputs: Vec<ExtraInput>,
    rust_channel: &str,
    rust_version: &str,
) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    let abs_project_path = PathBuf::from(project_path).canonicalize()?;
    let metadata_dir = abs_project_path.join(".repro-build");
    if !metadata_dir.exists() {
        tokio::fs::create_dir_all(&metadata_dir).await?;
    }
    let flake_path = metadata_dir.join("flake.nix");
    let flake_exists = tokio::fs::metadata(&flake_path).await.is_ok();
    if !flake_exists {
        generate_flake_file(&flake_path, &extra_inputs, rust_channel, rust_version).await?;
        println!("{}{}Generated flake.nix in {}{}", BOLD, GREEN, flake_path.display(), RESET);
    } else {
        println!("{}{}Using existing flake.nix at {}{}", BOLD, BLUE, flake_path.display(), RESET);
    }
    let container = setup_container(&docker, nix_image, &abs_project_path, &metadata_dir).await?;
    let cargo_lock_path = abs_project_path.join("Cargo.lock");
    let cargo_lock_exists = tokio::fs::metadata(&cargo_lock_path).await.is_ok();
    if !cargo_lock_exists {
        println!("{}{}Cargo.lock not found, generating it...{}", BOLD, YELLOW, RESET);
        execute_command(&docker, &container.id, "cargo generate-lockfile").await?;
    }
    let flake_lock_path = metadata_dir.join("flake.lock");
    let lock_exists = tokio::fs::metadata(&flake_lock_path).await.is_ok();
    if !lock_exists {
        generate_flake_lock(&docker, &container.id).await?;
        println!("{}{}Generated flake.lock{}", BOLD, GREEN, RESET);
    } else {
        println!("{}{}Using existing flake.lock{}", BOLD, BLUE, RESET);
    }
    
    execute_nix_build(&docker, &container.id, targets).await?;
    cleanup_container(&docker, &container.id).await?;
    Ok(())
}