use anyhow::Result;
use std::path::Path;
use tokio::fs;
use crate::{BOLD, YELLOW, RESET, GREEN};

/// Compare two files and return whether they are different
pub async fn files_differ<P1: AsRef<Path>, P2: AsRef<Path>>(path1: P1, path2: P2) -> Result<bool> {
    let content1 = fs::read_to_string(&path1).await?;
    let content2 = fs::read_to_string(&path2).await?;
    
    // Normalize line endings for comparison
    let normalized1 = content1.replace("\r\n", "\n").replace("\r", "\n");
    let normalized2 = content2.replace("\r\n", "\n").replace("\r", "\n");
    
    Ok(normalized1 != normalized2)
}

/// Compare generated flake.nix with existing one and warn if different
pub async fn check_flake_changes(
    generated_path: &Path,
    existing_path: &Path,
    generated_content: &str,
) -> Result<()> {
    if existing_path.exists() {
        let existing_content = fs::read_to_string(existing_path).await?;
        let normalized_existing = existing_content.replace("\r\n", "\n").replace("\r", "\n");
        let normalized_generated = generated_content.replace("\r\n", "\n").replace("\r", "\n");
        
        if normalized_existing != normalized_generated {
            println!("\n{}{}WARNING:{} Generated flake.nix differs from existing {}", 
                     BOLD, YELLOW, RESET, existing_path.display());
            println!("{}{}Differences detected in flake configuration.{}", BOLD, YELLOW, RESET);
            println!("Consider reviewing the changes and updating your flake.nix if needed.");
            println!("Generated flake.nix is available at: {}", generated_path.display());
        } else {
            println!("{}{}Generated flake.nix matches existing configuration.{}", BOLD, GREEN, RESET);
        }
    } else {
        println!("{}{}No existing flake.nix found at {}, using generated one.{}", 
                 BOLD, GREEN, RESET, existing_path.display());
    }
    
    Ok(())
}

/// Compare generated flake.lock with existing one and warn if different
pub async fn check_lock_changes(existing_lock_path: &Path, temp_lock_path: &Path) -> Result<()> {
    if existing_lock_path.exists() && temp_lock_path.exists() {
        match files_differ(existing_lock_path, temp_lock_path).await {
            Ok(true) => {
                println!("\n{}{}WARNING:{} Generated flake.lock differs from existing {}", 
                         BOLD, YELLOW, RESET, existing_lock_path.display());
                println!("{}{}Lock file changes detected.{}", BOLD, YELLOW, RESET);
                println!("This might indicate dependency updates or changes in flake inputs.");
                println!("Consider reviewing the lock file changes.");
            }
            Ok(false) => {
                println!("{}{}flake.lock is up to date.{}", BOLD, GREEN, RESET);
            }
            Err(e) => {
                println!("{}{}Warning:{} Failed to compare lock files: {}", BOLD, YELLOW, RESET, e);
            }
        }
    }
    
    Ok(())
} 