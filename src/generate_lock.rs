use anyhow::Result;
use bollard::Docker;
use crate::execute_command;
use crate::{RESET, BOLD, BLUE, RED};

pub async fn generate_flake_lock(
    docker: &Docker,
    container_id: &str,
) -> Result<()> {
    println!("{}{}Generating flake.lock file...{}", BOLD, BLUE, RESET);
    
    // Execute individual commands instead of chaining with &&
    
    // Configure git
    execute_command::execute_command(docker, container_id, "git config --global --add safe.directory /src").await?;
    
    // Add metadata directory
    execute_command::execute_command(docker, container_id, "git add .repro-build").await?;
    
    // Generate flake.lock
    execute_command::execute_command(docker, container_id, "nix --extra-experimental-features \"nix-command flakes\" flake lock ./.repro-build").await?;
    
    // Check if flake.lock was generated and set permissions
    let result = execute_command::execute_command(docker, container_id, "if [ -f .repro-build/flake.lock ]; then echo \"Successfully generated flake.lock file\"; chmod 666 .repro-build/flake.lock; else echo \"ERROR: Failed to generate flake.lock file\"; exit 1; fi").await;
    
    if result.is_err() {
        println!("{}{}Failed to generate flake.lock file{}", BOLD, RED, RESET);
    }
    
    result
}