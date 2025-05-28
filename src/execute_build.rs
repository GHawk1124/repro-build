use anyhow::{anyhow, Result};
use bollard::Docker;
use crate::execute_command::execute_command;
use crate::{RESET, BOLD, GREEN, RED, YELLOW, MAGENTA, BuildLogger};

/// Parse target name and determine build characteristics
fn parse_target(target: &str) -> (String, bool, bool) {
    // Returns (actual_target_name_for_flake, is_windows_msvc, is_static_musl)
    match target {
        "x86_64-linux-gnu" => (target.to_string(), false, false),
        "aarch64-linux-gnu" => (target.to_string(), false, false),
        "x86_64-linux-musl" => (target.to_string(), false, true),
        "aarch64-linux-musl" => (target.to_string(), false, true),
        "x86_64-w64-mingw32" => (target.to_string(), false, false),      // Windows GNU
        "x86_64-pc-windows-msvc" => (target.to_string(), true, false), // Windows MSVC
        "aarch64-w64-mingw32" => (target.to_string(), false, false),     // Windows ARM GNU
        _ => (target.to_string(), false, false), // Fallback, though should be caught by main.rs validation
    }
}

pub async fn execute_nix_build(
    docker: &Docker,
    container_id: &str,
    targets: &[&str],
    logger: &BuildLogger,
) -> Result<()> {
    let create_target_dir = "mkdir -p ./target/repro-build";
    let output = execute_command(docker, container_id, create_target_dir).await?;
    logger.log_command(create_target_dir, &output).await?;

    println!("{}{}Starting build process for {} target(s)...{}", BOLD, MAGENTA, targets.len(), RESET);
    logger.log(&format!("Starting build process for {} target(s)...", targets.len())).await?;

    let mut all_builds_successful = true;

    for target in targets {
        // Parse the target to get build characteristics
        let (clean_target, is_windows_msvc, _is_static_musl) = parse_target(target);

        println!("\n{}{}Building for target:{} {}", BOLD, MAGENTA, RESET, clean_target);
        logger.log(&format!("Building for target: {}", clean_target)).await?;

        // Main build command with sandbox option for Windows MSVC
        let sandbox_option = if is_windows_msvc { "--option sandbox false" } else { "" };

        // Run nix build
        let nix_build_cmd = format!(
            "nix --extra-experimental-features 'nix-command flakes' build {} ./.repro-build#{} --out-link ./result-{}",
            sandbox_option, clean_target, clean_target
        );

        let build_result = execute_command(docker, container_id, &nix_build_cmd).await;
        match build_result {
            Ok(output) => {
                logger.log_command(&nix_build_cmd, &output).await?;
            },
            Err(e) => {
                println!("{}{}Build failed for target {}:{} {}", BOLD, RED, clean_target, RESET, e);
                logger.log(&format!("Build failed for target {}: {}", clean_target, e)).await?;

                // Try to get more information about the build failure
                if let Ok(flake_content) = execute_command(docker, container_id, "cat .repro-build/flake.nix").await {
                    logger.log("Flake content for debugging:").await?;
                    logger.log(&flake_content).await?;
                }

                all_builds_successful = false;
                continue;
            }
        }

        // Check if the build produced any output
        let check_output_cmd = format!("if [ -L ./result-{0} ] && [ -e ./result-{0} ]; then echo \"true\"; else echo \"false\"; fi", clean_target);

        if let Ok(output) = execute_command(docker, container_id, &check_output_cmd).await {
            logger.log_command(&check_output_cmd, &output).await?;

            // Create target directory
            let mkdir_cmd = format!("mkdir -p ./target/repro-build/{}", clean_target);
            match execute_command(docker, container_id, &mkdir_cmd).await {
                Ok(output) => {
                    logger.log_command(&mkdir_cmd, &output).await?;
                },
                Err(e) => {
                    println!("{}{}Failed to create target directory:{} {}", BOLD, YELLOW, RESET, e);
                    logger.log(&format!("Failed to create target directory: {}", e)).await?;
                    continue;
                }
            }

            // Copy build artifacts using tar (handles Nix store permissions reliably)
            let copy_cmd = format!(
                "tar -C ./result-{} -cf - . | tar -C ./target/repro-build/{} -xf -",
                clean_target, clean_target
            );

            match execute_command(docker, container_id, &copy_cmd).await {
                Ok(output) => {
                    logger.log_command(&copy_cmd, &output).await?;
                    println!("{}{}Successfully copied build artifacts{}", BOLD, GREEN, RESET);
                },
                Err(e) => {
                    println!("{}{}Failed to copy build artifacts:{} {}", BOLD, YELLOW, RESET, e);
                    logger.log(&format!("Failed to copy build artifacts: {}", e)).await?;

                    // Fallback: try simple cp as last resort
                    let fallback_cmd = format!("cp -r ./result-{}/. ./target/repro-build/{}/", clean_target, clean_target);
                    match execute_command(docker, container_id, &fallback_cmd).await {
                        Ok(fallback_output) => {
                            logger.log_command(&fallback_cmd, &fallback_output).await?;
                            println!("{}{}Successfully copied using fallback method{}", BOLD, GREEN, RESET);
                        },
                        Err(_) => {
                            println!("{}{}Warning: Could not copy build artifacts, but build was successful{}", BOLD, YELLOW, RESET);
                        }
                    }
                }
            }

            // Cleanup result symlink
            let cleanup_cmd = format!("rm -rf ./result-{}", clean_target);
            match execute_command(docker, container_id, &cleanup_cmd).await {
                Ok(output) => {
                    logger.log_command(&cleanup_cmd, &output).await?;
                },
                Err(e) => {
                    println!("{}{}Failed to clean up symlink:{} {}", BOLD, YELLOW, RESET, e);
                    logger.log(&format!("Failed to clean up symlink: {}", e)).await?;
                }
            }

            println!("{}{}Build successful for target:{} {}", BOLD, GREEN, RESET, clean_target);
            logger.log(&format!("Build successful for target: {}", clean_target)).await?;
        } else {
            println!("{}{}Build produced no output for target:{} {}", BOLD, YELLOW, RESET, clean_target);
            logger.log(&format!("Build produced no output for target: {}", clean_target)).await?;
            all_builds_successful = false;
        }
    }

    if all_builds_successful {
        println!("\n{}{}All builds completed successfully!{}", BOLD, GREEN, RESET);
        logger.log("All builds completed successfully!").await?;
        Ok(())
    } else {
        println!("\n{}{}Some builds failed or produced no output{}", BOLD, YELLOW, RESET);
        logger.log("Some builds failed or produced no output").await?;
        Err(anyhow!("Not all builds were successful"))
    }
}