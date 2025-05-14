use anyhow::{anyhow, Result};
use bollard::Docker;
use futures_util::stream::StreamExt;

/// Helper function to execute a command in a container and stream the output
pub async fn execute_command(docker: &Docker, container_id: &str, cmd: &str) -> Result<()> {
    println!("Executing command: {}", cmd.lines().next().unwrap_or(cmd));
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
            return Err(anyhow!("Command failed with exit code {}: {}", exit_code, cmd));
        }
    }
    Ok(())
}