use anyhow::Result;
use clap::Parser;
use repro_build_lib::{build_with_nix, ExtraInput, RESET, BOLD, GREEN, RED, YELLOW, CYAN, MAGENTA};
use std::path::Path;

#[derive(Parser)]
#[command(name = "repro-build", about = "Cargo subcommand for Nix-based Rust builds")]
enum Cli {
    Build {
        #[arg(short, long, default_value = ".")]
        project: String,
        #[arg(short, long, default_value = "nixos/nix:latest")]
        image: String,
        #[arg(short, long, default_value = "x86_64-linux")]
        targets: String,
        #[arg(long, value_delimiter = ',')]
        extra: Vec<String>,
        #[arg(long, default_value = "stable", help = "Rust channel: stable or nightly")]
        rust_channel: String,
        #[arg(long, default_value = "latest", help = "Rust version, e.g. '1.75.0' or 'latest'")]
        rust_version: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse() {
        Cli::Build { project, image, targets, extra, rust_channel, rust_version } => {
            let project_path = Path::new(&project);
            if !project_path.exists() {
                eprintln!("{}{}ERROR:{} Project path '{}' does not exist", BOLD, RED, RESET, project);
                return Err(anyhow::anyhow!("Invalid project path"));
            }
            let cargo_path = project_path.join("Cargo.toml");
            if !cargo_path.exists() {
                eprintln!("{}{}ERROR:{} No Cargo.toml found in '{}' - is this a Rust project?", BOLD, RED, RESET, project);
                return Err(anyhow::anyhow!("Missing Cargo.toml"));
            }
            let extra_inputs: Vec<ExtraInput> = extra.iter().filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                if let (Some(name), Some(url)) = (parts.next(), parts.next()) {
                    Some(ExtraInput { name: name.to_string(), url: url.to_string() })
                } else { None }
            }).collect();
            let t: Vec<&str> = targets.split(',').collect();
            
            println!("{}{}Configuration:{}", BOLD, CYAN, RESET);
            println!("   - Project: {}", project);
            println!("   - Docker Image: {}", image);
            println!("   - Rust: {} {}", rust_channel, rust_version);
            println!("   - Targets: {:?}", t);
            println!("   - Extra inputs: {}", if extra_inputs.is_empty() { "none".to_string() } else { 
                extra_inputs.iter().map(|i| format!("{}={}", i.name, i.url)).collect::<Vec<_>>().join(", ") 
            });
            
            println!("\n{}{}Building project with Nix inside Docker...{}", BOLD, MAGENTA, RESET);
            
            let build_result = build_with_nix(&image, &project, &t, extra_inputs, &rust_channel, &rust_version).await;
            
            match build_result {
                Ok(_) => {
                    println!("\n{}{}Build completed successfully!{}", BOLD, GREEN, RESET);
                    let target_path = Path::new(&project).join("target/repro-build");
                    
                    if target_path.exists() {
                        println!("{}{}Build artifacts are available in:{}", BOLD, CYAN, RESET);
                        println!("   - target/repro-build/ directory");
                    } else {
                        println!("\n{}{}WARNING:{} No build artifacts found in target/repro-build", BOLD, YELLOW, RESET);
                        println!("This could indicate that all builds failed or no artifacts were produced");
                    }
                    Ok(())
                },
                Err(e) => {
                    eprintln!("\n{}{}Build failed:{} {}", BOLD, RED, RESET, e);
                    eprintln!("{}{}Troubleshooting tips:{}", BOLD, YELLOW, RESET);
                    eprintln!("   - Make sure Docker is running and your user has permission to access it");
                    eprintln!("   - Try running with the --image flag to use a different Nix image");
                    eprintln!("   - Check the error details above for more information");
                    Err(e)
                }
            }
        }
    }
}
