use anyhow::Result;
use bollard::Docker;
use serde::Serialize;
use std::path::PathBuf;
use std::collections::HashMap;

mod generate_flake;
mod generate_lock;
mod execute_command;
mod execute_build;
mod container_utils;
mod build_integration;
mod logging;

pub mod build_script {
    //! This module provides integration for build.rs scripts.
    //! 
    //! Use this to add reproducible builds to your project as a build dependency.
    
    pub use crate::build_integration::run_build;
}

pub use logging::BuildLogger;

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
    
    // Initialize logger
    let logger = BuildLogger::new(&metadata_dir).await?;
    println!("{}{}Logging to {}{}", BOLD, BLUE, logger.log_file().display(), RESET);
    
    // Log build configuration
    let mut config = HashMap::new();
    config.insert("Docker Image".to_string(), nix_image.to_string());
    config.insert("Project Path".to_string(), abs_project_path.display().to_string());
    config.insert("Targets".to_string(), targets.join(", "));
    config.insert("Rust Channel".to_string(), rust_channel.to_string());
    config.insert("Rust Version".to_string(), rust_version.to_string());
    config.insert("Build ID".to_string(), logger.build_id().to_string());
    
    logger.log_build_config(&config).await?;
    
    // Create flake.nix if it doesn't exist
    let flake_path = metadata_dir.join("flake.nix");
    let flake_exists = tokio::fs::metadata(&flake_path).await.is_ok();
    if !flake_exists {
        logger.log("Generating flake.nix file").await?;
        generate_flake_file(&flake_path, &extra_inputs, rust_channel, rust_version).await?;
        println!("{}{}Generated flake.nix in {}{}", BOLD, GREEN, flake_path.display(), RESET);
        logger.log(&format!("Generated flake.nix in {}", flake_path.display())).await?;
    } else {
        println!("{}{}Using existing flake.nix at {}{}", BOLD, BLUE, flake_path.display(), RESET);
        logger.log(&format!("Using existing flake.nix at {}", flake_path.display())).await?;
    }
    
    // Set up the Docker container
    logger.log("Setting up Docker container").await?;
    let container = setup_container(&docker, nix_image, &abs_project_path, &metadata_dir).await?;
    logger.log(&format!("Created container with ID: {}", container.id)).await?;

    // Configure git safe directory inside the container
    // This is crucial to run before any nix commands that might access .git history for flake inputs
    logger.log("Configuring git safe directory in container").await?;
    let git_config_cmd = "git config --global --add safe.directory /src";
    match execute_command(&docker, &container.id, git_config_cmd).await {
        Ok(output) => {
            logger.log_command(git_config_cmd, &output).await?;
        }
        Err(e) => {
            // Log the error but attempt to continue; some images might not have git or this might not be strictly necessary if not using git-based flake inputs directly from /src
            logger.log(&format!("Warning: Failed to set git safe.directory: {}. This might cause issues if your flake relies on git history from the source directory.", e)).await?;
            println!("{}{}Warning:{} Failed to set git safe.directory in container. Build might proceed if git history isn't needed for local flake inputs.", BOLD, YELLOW, RESET);
        }
    }
    
    // Generate Cargo.lock if needed
    let cargo_lock_path = abs_project_path.join("Cargo.lock");
    let cargo_lock_exists = tokio::fs::metadata(&cargo_lock_path).await.is_ok();
    if !cargo_lock_exists {
        println!("{}{}Cargo.lock not found, generating it...{}", BOLD, YELLOW, RESET);
        logger.log("Cargo.lock not found, generating it...").await?;
        let cmd = "cargo generate-lockfile";
        let output = execute_command(&docker, &container.id, cmd).await?;
        logger.log_command(cmd, &output).await?;
    }
    
    // Generate flake.lock if needed
    let flake_lock_path = metadata_dir.join("flake.lock");
    let lock_exists = tokio::fs::metadata(&flake_lock_path).await.is_ok();
    if !lock_exists {
        logger.log("Generating flake.lock file").await?;
        let output = generate_flake_lock(&docker, &container.id).await?;
        logger.log_command("nix flake lock", &output).await?;
        println!("{}{}Generated flake.lock{}", BOLD, GREEN, RESET);
    } else {
        println!("{}{}Using existing flake.lock{}", BOLD, BLUE, RESET);
        logger.log("Using existing flake.lock").await?;
    }
    
    // Execute the Nix build
    logger.log(&format!("Starting build for targets: {}", targets.join(", "))).await?;
    let build_result = execute_nix_build(&docker, &container.id, targets, &logger).await;
    
    // Clean up
    logger.log("Cleaning up container").await?;
    cleanup_container(&docker, &container.id).await?;
    
    // Log build completion
    let success = build_result.is_ok();
    logger.log_build_completion(success).await?;
    logger.flush().await?;
    
    // Return the build result
    build_result
}