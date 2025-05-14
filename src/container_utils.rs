use anyhow::Result;
use bollard::{container::{Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions}, Docker, image::CreateImageOptions};
use bollard::models::HostConfig;
use futures_util::stream::TryStreamExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Container info returned by setup_container
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
}

/// Set up and start a Docker container for Nix operations
pub async fn setup_container(
    docker: &Docker,
    nix_image: &str,
    project_path: &Path,
    metadata_dir: &Path,
) -> Result<ContainerInfo> {
    println!("Ensuring Nix image is available: {}", nix_image);
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
            format!("{}:/src:rw", project_path.display()),  // Mount project as read-write
            format!("{}:/flake-dir:rw", metadata_dir.display()),  // Mount metadata dir as writable
        ]),
        privileged: Some(true),
        ..Default::default()
    };
    let container_config = Config {
        image: Some(nix_image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "3600".to_string()]), // Keep container running
        working_dir: Some("/src".to_string()),  // Set working directory to /src
        host_config: Some(host_cfg),
        ..Default::default()
    };
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let container_name = format!("repro-build-{}", timestamp);
    let options = CreateContainerOptions::<String> { 
        name: container_name.clone(),
        platform: None,
    };
    println!("Starting Nix container...");
    let container = docker.create_container(Some(options), container_config).await?;
    docker.start_container(&container.id, None::<StartContainerOptions<String>>).await?;
    println!("Container started: {}", container_name);
    Ok(ContainerInfo {
        id: container.id,
        name: container_name,
    })
}

/// Clean up a Docker container
pub async fn cleanup_container(docker: &Docker, container_id: &str) -> Result<()> {
    println!("Cleaning up container...");
    docker.remove_container(container_id, Some(RemoveContainerOptions { 
        force: true,
        ..Default::default() 
    })).await?;
    Ok(())
} 