use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Represents a build logger
pub struct BuildLogger {
    log_file: PathBuf,
    build_id: String,
    log_buffer: Arc<Mutex<String>>,
}

impl BuildLogger {
    /// Create a new build logger with a unique ID
    pub async fn new(build_dir: &Path) -> Result<Self> {
        // Generate a unique build ID using UUID v4
        let build_id = Uuid::new_v4().to_string();
        
        // Create logs directory if it doesn't exist
        let logs_dir = build_dir.join("logs");
        tokio::fs::create_dir_all(&logs_dir).await?;
        
        // Create log file path with build ID
        let log_file = logs_dir.join(format!("build-{}.log", build_id));
        
        // Create and initialize the log file
        let mut file = File::create(&log_file).await?;
        
        // Write initial log header
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let header = format!(
            "=== repx Log ===\n\
             Build ID: {}\n\
             Timestamp: {}\n\
             =====================\n\n",
            build_id, timestamp
        );
        
        file.write_all(header.as_bytes()).await?;
        
        Ok(Self {
            log_file,
            build_id,
            log_buffer: Arc::new(Mutex::new(String::new())),
        })
    }
    
    /// Get the build ID
    pub fn build_id(&self) -> &str {
        &self.build_id
    }
    
    /// Get the log file path
    pub fn log_file(&self) -> &Path {
        &self.log_file
    }
    
    /// Log a message with timestamp
    pub async fn log(&self, message: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let log_entry = format!("[{}] {}\n", timestamp, message);
        
        // Add to buffer
        {
            let mut buffer = self.log_buffer.lock().unwrap();
            buffer.push_str(&log_entry);
        }
        
        // Write to file
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.log_file)
            .await?;
        
        file.write_all(log_entry.as_bytes()).await?;
        
        Ok(())
    }
    
    /// Log a command execution with its output
    pub async fn log_command(&self, command: &str, output: &str) -> Result<()> {
        let log_entry = format!(
            "Command: {}\nOutput:\n{}\n{}\n",
            command,
            output,
            "-".repeat(80)
        );
        
        self.log(&log_entry).await
    }
    
    /// Log build configuration
    pub async fn log_build_config(&self, config: &HashMap<String, String>) -> Result<()> {
        let mut config_str = String::from("Build Configuration:\n");
        
        for (key, value) in config {
            config_str.push_str(&format!("  {}: {}\n", key, value));
        }
        
        self.log(&config_str).await
    }
    
    /// Log build completion
    pub async fn log_build_completion(&self, success: bool) -> Result<()> {
        let status = if success { "SUCCESS" } else { "FAILURE" };
        let log_entry = format!(
            "\n=== Build Complete ===\n\
             Status: {}\n\
             =====================\n",
            status
        );
        
        self.log(&log_entry).await
    }
    
    /// Flush remaining logs to disk
    pub async fn flush(&self) -> Result<()> {
        let buffer = {
            let buffer = self.log_buffer.lock().unwrap();
            buffer.clone()
        };
        
        if !buffer.is_empty() {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&self.log_file)
                .await?;
            
            file.write_all(buffer.as_bytes()).await?;
        }
        
        Ok(())
    }
} 