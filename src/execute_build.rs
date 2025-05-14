use anyhow::Result;
use bollard::Docker;
use crate::execute_command::execute_command;

pub async fn execute_nix_build(
    docker: &Docker,
    container_id: &str,
    targets: &[&str],
) -> Result<()> {
    let create_target_dir = "mkdir -p ./target/repro-build";
    execute_command(docker, container_id, create_target_dir).await?;
    for target in targets {
        println!("Building for target: {}", target);
        let package_ref = if target.starts_with("packages.") {
            target.to_string()
        } else if *target == "default" || *target == "x86_64-linux" {
            format!("default")
        } else {
            format!("packages.{}", target)
        };
        let build_cmd = format!(
            "git config --global --add safe.directory /src && git add .repro-build && nix --extra-experimental-features 'nix-command flakes' build ./.repro-build#{} --out-link ./result-{} && mkdir -p ./target/repro-build/{} && cp -r ./result-{}/* ./target/repro-build/{}/ && rm -rf ./result-{}",
            package_ref, target, target, target, target, target
        );
        if let Err(e) = execute_command(docker, container_id, &build_cmd).await {
            println!("Warning: Build failed for target {}: {}", target, e);
            let parts: Vec<&str> = target.split('.').collect();
            let fallback_target = if parts.len() > 1 { parts[parts.len() - 1] } else { target };
            println!("Trying with simplified target name: {}", fallback_target);
            let fallback_cmd = format!(
                "git config --global --add safe.directory /src && git add .repro-build && nix --extra-experimental-features 'nix-command flakes' build ./.repro-build#{} --out-link ./result-{} && mkdir -p ./target/repro-build/{} && cp -r ./result-{}/* ./target/repro-build/{}/",
                fallback_target, target, target, target, target
            );
            execute_command(docker, container_id, &fallback_cmd).await?;
        }
    }
    Ok(())
} 