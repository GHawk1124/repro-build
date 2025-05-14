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

pub const FLAKE_TEMPLATE: &'static str = include_str!("../templates/flake.nix.hbs");

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
        generate_flake_file(&flake_path, &extra_inputs).await?;
        println!("Generated flake.nix in {}", flake_path.display());
    } else {
        println!("Using existing flake.nix at {}", flake_path.display());
    }
    let container = setup_container(&docker, nix_image, &abs_project_path, &metadata_dir).await?;
    let flake_lock_path = metadata_dir.join("flake.lock");
    let lock_exists = tokio::fs::metadata(&flake_lock_path).await.is_ok();
    if !lock_exists {
        generate_flake_lock(&docker, &container.id).await?;
        println!("Generated flake.lock");
    } else {
        println!("Using existing flake.lock");
    }
    execute_nix_build(&docker, &container.id, targets).await?;
    cleanup_container(&docker, &container.id).await?;
    Ok(())
}