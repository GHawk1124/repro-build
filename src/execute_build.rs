use anyhow::{anyhow, Result};
use bollard::Docker;
use crate::execute_command::execute_command;
use crate::{RESET, BOLD, GREEN, RED, YELLOW, MAGENTA};

pub async fn execute_nix_build(
    docker: &Docker,
    container_id: &str,
    targets: &[&str],
) -> Result<()> {
    let create_target_dir = "mkdir -p ./target/repro-build";
    execute_command(docker, container_id, create_target_dir).await?;
    
    println!("{}{}Starting build process for {} target(s)...{}", BOLD, MAGENTA, targets.len(), RESET);
    
    let mut all_builds_successful = true;
    
    for target in targets {
        // Remove "packages." prefix if it exists
        let clean_target = if target.starts_with("packages.") {
            &target["packages.".len()..]
        } else {
            target
        };
        
        println!("\n{}{}Building for target:{} {}", BOLD, MAGENTA, RESET, clean_target);
        
        // Check if target is Windows MSVC
        let is_windows_msvc = clean_target.contains("windows") && clean_target.contains("msvc");
        
        // Main build command with sandbox option for Windows MSVC
        let sandbox_option = if is_windows_msvc { "--option sandbox false" } else { "" };
        
        // Execute each command individually instead of using &&
        
        // Configure git
        if let Err(e) = execute_command(docker, container_id, "git config --global --add safe.directory /src").await {
            println!("{}{}Git configuration failed:{} {}", BOLD, RED, RESET, e);
            all_builds_successful = false;
            continue;
        }
        
        // Add metadata directory
        if let Err(e) = execute_command(docker, container_id, "git add .repro-build").await {
            println!("{}{}Failed to add metadata directory:{} {}", BOLD, RED, RESET, e);
            all_builds_successful = false;
            continue;
        }
        
        // Run nix build
        let nix_build_cmd = format!(
            "nix --extra-experimental-features 'nix-command flakes' build {} ./.repro-build#{} --out-link ./result-{}",
            sandbox_option, clean_target, clean_target
        );
        
        if let Err(e) = execute_command(docker, container_id, &nix_build_cmd).await {
            println!("{}{}Build failed for target {}:{} {}", BOLD, RED, clean_target, RESET, e);
            all_builds_successful = false;
            
            // Try to get more information about the build failure
            let _ = execute_command(docker, container_id, "cat .repro-build/flake.nix").await;
            
            continue;
        }
        
        // Check if the build produced any output
        let check_output_cmd = format!("if [ -L ./result-{0} ] && [ -e ./result-{0} ]; then echo \"true\"; else echo \"false\"; fi", clean_target);
        
        if let Ok(output) = execute_command(docker, container_id, &check_output_cmd).await {
            // Create target directory
            let mkdir_cmd = format!("mkdir -p ./target/repro-build/{}", clean_target);
            if let Err(e) = execute_command(docker, container_id, &mkdir_cmd).await {
                println!("{}{}Failed to create target directory:{} {}", BOLD, YELLOW, RESET, e);
                continue;
            }
            
            // Copy build artifacts
            let copy_cmd = format!("cp -r ./result-{}/* ./target/repro-build/{}/", clean_target, clean_target);
            if let Err(e) = execute_command(docker, container_id, &copy_cmd).await {
                println!("{}{}Failed to copy build artifacts:{} {}", BOLD, YELLOW, RESET, e);
                continue;
            }
            
            // Cleanup result symlink
            let cleanup_cmd = format!("rm -rf ./result-{}", clean_target);
            if let Err(e) = execute_command(docker, container_id, &cleanup_cmd).await {
                println!("{}{}Failed to clean up symlink:{} {}", BOLD, YELLOW, RESET, e);
            }
            
            println!("{}{}Build successful for target:{} {}", BOLD, GREEN, RESET, clean_target);
        } else {
            println!("{}{}Build produced no output for target:{} {}", BOLD, YELLOW, RESET, clean_target);
            all_builds_successful = false;
        }
    }
    
    if all_builds_successful {
        println!("\n{}{}All builds completed successfully!{}", BOLD, GREEN, RESET);
        Ok(())
    } else {
        println!("\n{}{}Some builds failed or produced no output{}", BOLD, YELLOW, RESET);
        Err(anyhow!("Not all builds were successful"))
    }
} 