use anyhow::{anyhow, Result};
use bollard::Docker;
use futures_util::stream::StreamExt;

pub async fn generate_flake_lock(
    docker: &Docker,
    container_id: &str,
) -> Result<()> {
    println!("Generating flake.lock file...");
    let commands = vec![
        "echo \"Generating flake.lock file...\"",
        "git config --global --add safe.directory /src && git add .repro-build && nix --extra-experimental-features \"nix-command flakes\" flake lock ./.repro-build",
        "if [ -f .repro-build/flake.lock ]; then\n  echo \"Successfully generated flake.lock file\"\n  chmod 666 .repro-build/flake.lock\n  ls -la .repro-build/flake.lock\nelse\n  echo \"ERROR: Failed to generate flake.lock file\"\n  exit 1\nfi",
    ];
    for cmd in commands {
        let exec_options = bollard::exec::CreateExecOptions {
            cmd: Some(vec!["sh", "-c", cmd]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/src"),
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
        let exec_inspect = docker.inspect_exec(&exec.id).await?;
        if let Some(exit_code) = exec_inspect.exit_code {
            if exit_code != 0 {
                return Err(anyhow!("Command failed with exit code {}", exit_code));
            }
        }
    }
    Ok(())
}