use anyhow::{anyhow, Result};
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use crate::{RESET, BOLD, GREEN, RED, YELLOW, BLUE, CYAN};

/// Helper function to execute a command in a container and stream the output
pub async fn execute_command(docker: &Docker, container_id: &str, cmd: &str) -> Result<String> {
    let cmd_summary = cmd.lines().next().unwrap_or(cmd);
    let display_cmd = if cmd_summary.len() > 70 { 
        format!("{}...", &cmd_summary[..67]) 
    } else { 
        cmd_summary.to_string() 
    };
    print!("{}{}Executing:{} {} ", BOLD, BLUE, RESET, display_cmd);
    stdout().flush()?;
    let exec_options = bollard::exec::CreateExecOptions {
        cmd: Some(vec!["sh", "-c", cmd]),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        working_dir: Some("/app"),
        ..Default::default()
    };
    let exec = docker.create_exec(container_id, exec_options).await?;
    let started_exec = docker.start_exec(&exec.id, None).await?;
    
    let mut full_output = String::new();
    
    if let bollard::exec::StartExecResults::Attached { mut output, .. } = started_exec {
        let mut last_progress = String::new();
        let mut full_logs = Vec::new();
        let mut last_update = Instant::now();
        let update_interval = Duration::from_millis(500); // Increase interval
        let mut important_message_count = 0;
        let mut error_messages = Vec::new();
        let mut last_displayed_count = 0;
        
        while let Some(Ok(output_chunk)) = output.next().await {
            match output_chunk {
                bollard::container::LogOutput::StdOut { message } | 
                bollard::container::LogOutput::StdErr { message } => {
                    let message_str = std::str::from_utf8(&message)?;
                    full_logs.push(message_str.to_string());
                    full_output.push_str(message_str);
                    
                    // Capture error messages for better reporting
                    if message_str.contains("error:") {
                        error_messages.push(message_str.trim().to_string());
                    }
                    
                    // For messages about copying from cache, count them but don't display individually
                    if message_str.contains("copying path") {
                        important_message_count += 1;
                        
                        // Only update the counter at intervals AND if count changed significantly
                        if last_update.elapsed() >= update_interval && 
                           (important_message_count - last_displayed_count) >= 10 {
                            print!("\r\x1B[K{}{}Executing:{} {} {}{}(copied {} paths){}", 
                                BOLD, BLUE, RESET, display_cmd, 
                                CYAN, BOLD, important_message_count, RESET);
                            stdout().flush()?;
                            last_update = Instant::now();
                            last_displayed_count = important_message_count;
                        }
                        continue;
                    }
                    
                    // Determine if this is an important progress message
                    let is_important = message_str.contains("evaluating") || 
                                       message_str.contains("building") || 
                                       message_str.contains("downloading") || 
                                       message_str.contains("fetching") ||
                                       message_str.contains("error") ||
                                       message_str.contains("warning");
                    
                    if is_important && last_update.elapsed() >= update_interval {
                        let lines: Vec<&str> = message_str.lines().collect();
                        if !lines.is_empty() {
                            let progress_line = lines[lines.len() - 1].trim();
                            if !progress_line.is_empty() && progress_line != last_progress {
                                last_progress = progress_line.to_string();
                                let max_progress_len = if display_cmd.len() > 30 { 40 } else { 60 };
                                let trimmed_progress = if progress_line.len() > max_progress_len {
                                    format!("{}...", &progress_line[..max_progress_len-3])
                                } else {
                                    progress_line.to_string()
                                };
                                let color = if progress_line.contains("error") {
                                    RED
                                } else if progress_line.contains("warning") {
                                    YELLOW
                                } else if progress_line.contains("building") {
                                    GREEN
                                } else {
                                    CYAN
                                };
                                print!("\r\x1B[K{}{}Executing:{} {} {}{}{}{}", 
                                    BOLD, BLUE, RESET, display_cmd, 
                                    BOLD, color, trimmed_progress, RESET);
                                stdout().flush()?;
                                last_update = Instant::now();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        // Clear the current line
        print!("\r\x1B[K");
        
        let exec_inspect = docker.inspect_exec(&exec.id).await?;
        if let Some(exit_code) = exec_inspect.exit_code {
            if exit_code != 0 {
                println!("{}{}Command failed with exit code {}:{} {}", BOLD, RED, exit_code, RESET, cmd);
                
                // Print captured error messages
                if !error_messages.is_empty() {
                    println!("{}{}Error details:{}", BOLD, RED, RESET);
                    for err in error_messages.iter().take(5) { // Limit to 5 errors
                        println!("  {}", err);
                    }
                    if error_messages.len() > 5 {
                        println!("  ... and {} more errors", error_messages.len() - 5);
                    }
                } else {
                    // If no specific error messages found, look for relevant lines in the full logs
                    let error_context = full_logs.iter()
                        .flat_map(|log| log.lines())
                        .filter(|line| line.contains("error") || line.contains("failed"))
                        .take(5)
                        .collect::<Vec<_>>();
                    
                    if !error_context.is_empty() {
                        println!("{}{}Error context:{}", BOLD, RED, RESET);
                        for line in error_context {
                            println!("  {}", line.trim());
                        }
                    }
                }
                
                return Err(anyhow!("Command failed with exit code {}", exit_code));
            } else {
                // Print success message on completion
                println!("{}{}Completed:{} {}", BOLD, GREEN, RESET, display_cmd);
            }
        }
    } else {
        return Err(anyhow!("Failed to start command execution"));
    }
    
    Ok(full_output)
}