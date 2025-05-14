use anyhow::Result;
use clap::Parser;
use repro_build_lib::{build_with_nix, ExtraInput};
use std::path::Path;

#[derive(Parser)]
#[command(name = "repro-build", about = "Cargo subcommand for Nix-based Rust builds")]
enum Cli {
    /// Build project with Nix inside Docker (everything happens in container)
    Build {
        /// Project path (default: ".")
        #[arg(short, long, default_value = ".")]
        project: String,
        
        /// Nix image tag
        #[arg(short, long, default_value = "nixos/nix:2.28.3")]
        image: String,
        
        /// Target platforms to build for, comma-separated
        #[arg(short, long, default_value = "x86_64-unknown-linux-gnu")]
        targets: String,
        
        /// Additional Nix inputs in name=URL format, comma-separated
        #[arg(long, value_delimiter = ',')]
        extra: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse() {
        Cli::Build { project, image, targets, extra } => {
            // Validate the project path exists
            let project_path = Path::new(&project);
            if !project_path.exists() {
                eprintln!("Error: Project path '{}' does not exist", project);
                return Err(anyhow::anyhow!("Invalid project path"));
            }
            
            // Check for Cargo.toml in project directory
            let cargo_path = project_path.join("Cargo.toml");
            if !cargo_path.exists() {
                eprintln!("Error: No Cargo.toml found in '{}' - is this a Rust project?", project);
                return Err(anyhow::anyhow!("Missing Cargo.toml"));
            }
            
            let extra_inputs: Vec<ExtraInput> = extra.iter().filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                if let (Some(name), Some(url)) = (parts.next(), parts.next()) {
                    Some(ExtraInput { name: name.to_string(), url: url.to_string() })
                } else { None }
            }).collect();
            
            let t: Vec<&str> = targets.split(',').collect();
            
            println!("Building project with Nix inside Docker...");
            match build_with_nix(&image, &project, &t, extra_inputs).await {
                Ok(_) => {
                    println!("Build completed successfully!");
                    
                    // Check for the output directories
                    let target_path = Path::new(&project).join("target/repro-build");
                    let result_path = Path::new(&project).join("result");
                    
                    if !target_path.exists() && !result_path.exists() {
                        eprintln!("\nWARNING: Expected output directories not found after build");
                        eprintln!("This could indicate an issue with the build process or file permissions");
                        eprintln!("See container logs above for more details");
                    } else {
                        println!("Build artifacts are available in:");
                        if target_path.exists() {
                            println!("- target/repro-build/ directory (recommended)");
                        }
                        if result_path.exists() {
                            println!("- result/ directory (for backward compatibility)");
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Build failed: {}", e);
                    eprintln!("\nTroubleshooting tips:");
                    eprintln!("1. Make sure Docker is running and your user has permission to access it");
                    eprintln!("2. Try running with the --image flag to use a different Nix image");
                    eprintln!("3. Check the container logs above for more details");
                    return Err(e);
                }
            }
        }
    }
    
    Ok(())
}
