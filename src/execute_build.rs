use anyhow::Result;
use bollard::Docker;

use crate::execute_command::execute_command;

/// Execute the Nix build in a Docker container
pub async fn execute_nix_build(
    docker: &Docker,
    container_id: &str,
    targets: &[&str],
) -> Result<()> {
    println!("Running Nix build...");
    
    // Set up Nix environment and build for each target
    let setup_cmd = "export NIX_CONFIG=\"experimental-features = nix-command flakes\"";
    execute_command(docker, container_id, setup_cmd).await?;
    
    // Create target directory if it doesn't exist
    let create_target_dir = "mkdir -p /src/target/repro-build";
    execute_command(docker, container_id, create_target_dir).await?;
    
    // Build for each target
    for target in targets {
        println!("Building for target: {}", target);
        
        // Prepare build command with target
        let build_cmd = format!(
            "nix build -L .#default --target {} --out-link /src/result-{} && mkdir -p /src/target/repro-build/{} && cp -r /src/result-{}/* /src/target/repro-build/{}/",
            target, target, target, target, target
        );
        
        // Execute the build
        if let Err(e) = execute_command(docker, container_id, &build_cmd).await {
            println!("Warning: Build failed for target {}: {}", target, e);
            
            // Try a simplified build command that might work better
            let fallback_cmd = "nix build -L .#default";
            println!("Trying fallback build command...");
            execute_command(docker, container_id, fallback_cmd).await?;
        }
    }
    
    Ok(())
} 