use anyhow::{anyhow, Result};
use bollard::Docker;
use futures_util::stream::StreamExt;

/// Generate a flake.lock file by using an existing container
pub async fn generate_flake_lock(
    docker: &Docker,
    container_id: &str,
) -> Result<()> {
    println!("Generating flake.lock file...");
    
    // Series of commands to execute in the container
    let commands = vec![
        // Enable Nix experimental features
        "export NIX_CONFIG=\"experimental-features = nix-command flakes\"",
        
        // Show current directory and contents
        "echo \"Current directory: $(pwd)\"",
        "echo \"Directory contents:\" && ls -la",
        
        // Extract package metadata
        "echo \"Extracting package metadata...\"",
        "PACKAGE_NAME=$(cd /src && nix shell nixpkgs#cargo nixpkgs#jq -c bash -c 'cargo metadata --no-deps --format-version 1 | jq -r \".packages[0].name\"' 2>/dev/null || echo \"project\")",
        "PACKAGE_VERSION=$(cd /src && nix shell nixpkgs#cargo nixpkgs#jq -c bash -c 'cargo metadata --no-deps --format-version 1 | jq -r \".packages[0].version\"' 2>/dev/null || echo \"0.1.0\")",
        
        "echo \"Package details: $PACKAGE_NAME v$PACKAGE_VERSION\"",
        
        // Update the flake.nix with package details
        "if [ -n \"$PACKAGE_NAME\" ] && [ -n \"$PACKAGE_VERSION\" ]; then\n  echo \"Updating flake.nix with: $PACKAGE_NAME v$PACKAGE_VERSION\"\n  sed -i \"s/pname = \\\"project\\\"/pname = \\\"$PACKAGE_NAME\\\"/\" flake.nix\n  sed -i \"s/version = \\\"0.1.0\\\"/version = \\\"$PACKAGE_VERSION\\\"/\" flake.nix\n  sed -i \"s/Rust flake for project/Rust flake for $PACKAGE_NAME/\" flake.nix\nelse\n  echo \"Warning: Unable to extract package name/version, using defaults\"\nfi",
        
        // Generate the flake.lock file
        "echo \"Generating flake.lock file...\"",
        "nix --extra-experimental-features \"nix-command flakes\" flake lock --no-update-lock-file || {\n  echo \"Error during flake.lock generation, trying with update allowed...\"\n  nix --extra-experimental-features \"nix-command flakes\" flake lock\n}",
        
        // Verify the lock file was created
        "if [ -f flake.lock ]; then\n  echo \"Successfully generated flake.lock file\"\n  chmod 666 flake.lock\n  ls -la flake.lock\nelse\n  echo \"ERROR: Failed to generate flake.lock file\"\n  exit 1\nfi",
    ];
    
    // Execute each command in sequence
    for cmd in commands {
        let exec_options = bollard::exec::CreateExecOptions {
            cmd: Some(vec!["sh", "-c", cmd]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/flake-dir"),
            ..Default::default()
        };
        
        let exec = docker.create_exec(container_id, exec_options).await?;
        
        let started_exec = docker.start_exec(&exec.id, None).await?;
        
        if let bollard::exec::StartExecResults::Attached { mut output, .. } = started_exec {
            while let Some(Ok(output_chunk)) = output.next().await {
                match output_chunk {
                    bollard::container::LogOutput::StdOut { message } => {
                        print!("{}", std::str::from_utf8(&message)?);
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        eprint!("{}", std::str::from_utf8(&message)?);
                    }
                    _ => {}
                }
            }
        }
        
        // Check execution results
        let exec_inspect = docker.inspect_exec(&exec.id).await?;
        if let Some(exit_code) = exec_inspect.exit_code {
            if exit_code != 0 {
                return Err(anyhow!("Command failed with exit code {}", exit_code));
            }
        }
    }
    
    Ok(())
}