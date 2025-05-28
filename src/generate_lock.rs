use anyhow::Result;
use bollard::Docker;
use crate::execute_command::execute_command;

/// Generate flake.lock file inside the container
pub async fn generate_flake_lock(docker: &Docker, container_id: &str) -> Result<String> {
    let cmd = "cd .repx && nix --extra-experimental-features 'nix-command flakes' flake lock";
    let output = execute_command(docker, container_id, cmd).await?;
    Ok(output)
}