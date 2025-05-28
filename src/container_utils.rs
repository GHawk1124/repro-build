use anyhow::Result;
use bollard::{container::{Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions}, Docker, image::CreateImageOptions};
use bollard::models::HostConfig;
use futures_util::stream::TryStreamExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// Import color constants from lib.rs
use crate::{RESET, BOLD, GREEN, BLUE, CYAN};

/// Convert a Windows path to a Docker-compatible format
fn windows_path_to_docker(path: &Path) -> String {
    let path_str = path.display().to_string();
    
    // Handle Windows extended path format (\\?\)
    if path_str.starts_with("\\\\?\\") {
        let cleaned = &path_str[4..]; // Remove \\?\
        
        // Convert Windows drive letter to Unix-style path for Docker
        if cleaned.len() >= 3 && cleaned.chars().nth(1) == Some(':') {
            let drive = cleaned.chars().nth(0).unwrap().to_ascii_lowercase();
            let rest = &cleaned[2..].replace('\\', "/");
            format!("/{}{}", drive, rest)
        } else {
            cleaned.replace('\\', "/")
        }
    } else {
        // Handle regular Windows paths
        if cfg!(windows) && path_str.len() >= 3 && path_str.chars().nth(1) == Some(':') {
            let drive = path_str.chars().nth(0).unwrap().to_ascii_lowercase();
            let rest = &path_str[2..].replace('\\', "/");
            format!("/{}{}", drive, rest)
        } else {
            path_str
        }
    }
}

/// Container info returned by setup_container
#[derive(Debug)]
pub struct ContainerInfo {
    pub id: String,
    #[allow(dead_code)]
    pub name: String,
}

/// Set up and start a Docker container for Nix operations
pub async fn setup_container(
    docker: &Docker,
    nix_image: &str,
    project_path: &Path,
    metadata_dir: &Path,
) -> Result<ContainerInfo> {
    println!("{}{}Ensuring Nix image is available:{} {}", BOLD, BLUE, RESET, nix_image);
    docker.create_image(
        Some(CreateImageOptions {
            from_image: nix_image.to_string(),
            ..Default::default()
        }),
        None,
        None,
    ).try_collect::<Vec<_>>().await?;
    let host_cfg = HostConfig {
        binds: Some(vec![
            format!("{}:/app:rw", windows_path_to_docker(project_path)),  // Mount project as read-write
            format!("{}:/flake-dir:rw", windows_path_to_docker(metadata_dir)),  // Mount metadata dir as writable
        ]),
        privileged: Some(true),
        ..Default::default()
    };
    let container_config = Config {
        image: Some(nix_image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "3600".to_string()]), // Keep container running
        working_dir: Some("/app".to_string()),  // Set working directory to /app
        host_config: Some(host_cfg),
        ..Default::default()
    };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let container_name = format!("repx-{}", timestamp);
    let options = CreateContainerOptions::<String> { 
        name: container_name.clone(),
        platform: None,
    };
    println!("{}{}Starting Nix container...{}", BOLD, BLUE, RESET);
    let container = docker.create_container(Some(options), container_config).await?;
    docker.start_container(&container.id, None::<StartContainerOptions<String>>).await?;
    println!("{}{}Container started:{} {}", BOLD, GREEN, RESET, container_name);
    Ok(ContainerInfo {
        id: container.id,
        name: container_name,
    })
}

/// Clean up a Docker container
pub async fn cleanup_container(docker: &Docker, container_id: &str) -> Result<()> {
    println!("{}{}Cleaning up container...{}", BOLD, CYAN, RESET);
    docker.remove_container(container_id, Some(RemoveContainerOptions { 
        force: true,
        ..Default::default() 
    })).await?;
    println!("{}{}Container removed successfully{}", BOLD, GREEN, RESET);
    Ok(())
} 