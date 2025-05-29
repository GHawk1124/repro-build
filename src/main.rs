use anyhow::Result;
use clap::Parser;
use cargo_metadata::MetadataCommand;
use repx_lib::{build_with_nix, RepxConfig, RESET, BOLD, GREEN, RED, YELLOW, CYAN, MAGENTA};
use std::path::Path;
use tokio::fs;

#[derive(Parser)]
#[command(name = "repx", about = "Cargo subcommand for Nix-based Rust builds")]
enum Cli {
    Build {
        #[arg(short, long, help = "Path location to your Cargo.toml or project root.")]
        project: Option<String>,
        #[arg(short, long, help = "Pin nix docker image to a specific version.")]
        image: Option<String>,
        #[arg(short, long, help = "Comma-separated list of targets to build for. If not specified, builds for host target.")]
        targets: Option<String>,
        #[arg(long, help = "List all available targets and exit")]
        list_targets: bool,
        #[arg(long, value_delimiter = ',', help = "Extra packages to install with nix.")]
        extra: Option<Vec<String>>,
        #[arg(long, help = "Rust channel: stable or nightly")]
        rust_channel: Option<String>,
        #[arg(long, help = "Rust version, e.g. '1.75.0' or 'latest'")]
        rust_version: Option<String>,
        #[arg(long, help = "nixpkgs URL/commit to use for reproducible builds")]
        nixpkgs_url: Option<String>,
        #[arg(short = 'c', long, help = "Path to repx.toml configuration file")]
        config: Option<String>,
    },
    #[command(about = "Initialize a new repx.toml configuration file")]
    Init {
        #[arg(short, long, help = "Force overwrite existing repx.toml")]
        force: bool,
    },
    #[command(about = "Clean target and .repx directories")]
    Clean {
        #[arg(short, long, default_value = ".", help = "Path location to your project root.")]
        project: String,
    },
    #[command(about = "Print the repx version")]
    Release,
}

// Available targets based on the flake template
const AVAILABLE_TARGETS: &[&str] = &[
    "x86_64-linux-gnu",
    "aarch64-linux-gnu",
    "x86_64-linux-musl",
    "aarch64-linux-musl",
    "x86_64-w64-mingw32",       // Windows GNU
    "x86_64-pc-windows-msvc", // Windows MSVC
    "aarch64-w64-mingw32",      // Windows ARM GNU (experimental)
    "x86_64-apple-darwin",      // macOS Intel
    "aarch64-apple-darwin",     // macOS Apple Silicon
];

fn get_host_target() -> &'static str {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        "x86_64-linux-gnu"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    {
        "aarch64-linux-gnu"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
    {
        "x86_64-w64-mingw32" // Default to GNU for Windows host
    }
    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "linux"),
        all(target_arch = "aarch64", target_os = "linux"),
        all(target_arch = "x86_64", target_os = "windows"),
        all(target_arch = "x86_64", target_os = "macos"),
        all(target_arch = "aarch64", target_os = "macos")
    )))]
    {
        "x86_64-linux-gnu"  // Default fallback
    }
}

fn print_available_targets() {
    println!("{}{}Available targets:{}", BOLD, CYAN, RESET);
    for target in AVAILABLE_TARGETS {
        let description = match *target {
            "x86_64-linux-gnu" => "Linux x86_64 (GNU libc, dynamic)",
            "aarch64-linux-gnu" => "Linux ARM64/AArch64 (GNU libc, dynamic)",
            "x86_64-linux-musl" => "Linux x86_64 (musl libc, static)",
            "aarch64-linux-musl" => "Linux ARM64/AArch64 (musl libc, static)",
            "x86_64-w64-mingw32" => "Windows x86_64 (MinGW-w64/GNU)",
            "x86_64-pc-windows-msvc" => "Windows x86_64 (MSVC toolchain)",
            "aarch64-w64-mingw32" => "Windows ARM64 (MinGW-w64/GNU, experimental)",
            "x86_64-apple-darwin" => "macOS x86_64 (Intel)",
            "aarch64-apple-darwin" => "macOS ARM64 (Apple Silicon)",
            _ => "Unknown target",
        };
        println!("   - {}: {}", target, description);
    }
}

fn print_version() -> Result<()> {
    let metadata = MetadataCommand::new().exec()?;

    // Find the repx package in the metadata
    let repx_package = metadata.packages
        .iter()
        .find(|p| p.name.as_str() == "repx")
        .ok_or_else(|| anyhow::anyhow!("Could not find repx package in metadata"))?;

    println!("{}{}repx version:{} {}", BOLD, CYAN, RESET, repx_package.version);
    Ok(())
}

async fn load_config(config_path: Option<String>) -> Result<RepxConfig> {
    let config_file = config_path.as_deref().unwrap_or(RepxConfig::default_config_path());
    
    if Path::new(config_file).exists() {
        println!("{}{}Loading configuration from:{} {}", BOLD, CYAN, RESET, config_file);
        RepxConfig::from_file(config_file).await
    } else if config_path.is_some() {
        // If a specific config file was requested but doesn't exist, that's an error
        return Err(anyhow::anyhow!("Configuration file '{}' not found", config_file));
    } else {
        // Use default configuration if no config file exists
        Ok(RepxConfig::default())
    }
}

fn merge_config_with_args(mut config: RepxConfig, args: &Cli) -> RepxConfig {
    if let Cli::Build { 
        project, image, targets, extra, rust_channel, rust_version, nixpkgs_url, .. 
    } = args {
        if let Some(ref p) = project {
            config.project = p.clone();
        }
        if let Some(ref i) = image {
            config.image = i.clone();
        }
        if let Some(ref t) = targets {
            config.targets = Some(t.clone());
        }
        if let Some(ref e) = extra {
            config.extra = e.clone();
        }
        if let Some(ref rc) = rust_channel {
            config.rust_channel = rc.clone();
        }
        if let Some(ref rv) = rust_version {
            config.rust_version = rv.clone();
        }
        if let Some(ref nu) = nixpkgs_url {
            config.nixpkgs_url = nu.clone();
        }
    }
    config
}

async fn clean_directories(project_path: &str) -> Result<()> {
    let project = Path::new(project_path);
    let target_dir = project.join("target");
    let repx_dir = project.join(".repx");
    
    let mut cleaned = Vec::new();
    
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).await?;
        cleaned.push("target/");
    }
    
    if repx_dir.exists() {
        fs::remove_dir_all(&repx_dir).await?;
        cleaned.push(".repx/");
    }
    
    if cleaned.is_empty() {
        println!("{}{}No directories to clean.{}", BOLD, YELLOW, RESET);
    } else {
        println!("{}{}Cleaned directories:{} {}", BOLD, GREEN, RESET, cleaned.join(", "));
    }
    
    Ok(())
}

async fn init_config(force: bool) -> Result<()> {
    let config_path = RepxConfig::default_config_path();
    
    if Path::new(config_path).exists() && !force {
        println!("{}{}Configuration file already exists:{} {}", BOLD, YELLOW, RESET, config_path);
        println!("Use --force to overwrite the existing file.");
        return Ok(());
    }
    
    let config = RepxConfig::default();
    config.to_file(config_path).await?;
    
    println!("{}{}Created configuration file:{} {}", BOLD, GREEN, RESET, config_path);
    println!("Edit the file to customize your build settings.");
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match &cli {
        Cli::Build { list_targets, config, .. } => {
            if *list_targets {
                print_available_targets();
                return Ok(());
            }

            // Load configuration from file if it exists
            let base_config = load_config(config.clone()).await?;
            
            // Merge with command line arguments
            let final_config = merge_config_with_args(base_config, &cli);

            let project_path = Path::new(&final_config.project);
            if !project_path.exists() {
                eprintln!("{}{}ERROR:{} Project path '{}' does not exist", BOLD, RED, RESET, final_config.project);
                return Err(anyhow::anyhow!("Invalid project path"));
            }
            let cargo_path = project_path.join("Cargo.toml");
            if !cargo_path.exists() {
                eprintln!("{}{}ERROR:{} No Cargo.toml found in '{}' - is this a Rust project?", BOLD, RED, RESET, final_config.project);
                return Err(anyhow::anyhow!("Missing Cargo.toml"));
            }
            
            // Determine targets to build
            let target_string = match final_config.targets {
                Some(t) => t,
                None => {
                    let host_target = get_host_target();
                    println!("{}{}INFO:{} No targets specified, building for host target: {}", BOLD, CYAN, RESET, host_target);
                    host_target.to_string()
                }
            };
            let t: Vec<&str> = target_string.split(',').collect();

            // Validate targets
            for target in &t {
                if !AVAILABLE_TARGETS.contains(target) {
                    eprintln!("{}{}ERROR:{} Unknown target '{}'. Use --list-targets to see available targets.", BOLD, RED, RESET, target);
                    return Err(anyhow::anyhow!("Invalid target: {}", target));
                }
            }

            println!("{}{}Configuration:{}", BOLD, CYAN, RESET);
            println!("   - Project: {}", final_config.project);
            println!("   - Docker Image: {}", final_config.image);
            println!("   - Rust: {} {}", final_config.rust_channel, final_config.rust_version);
            println!("   - nixpkgs: {}", final_config.nixpkgs_url);
            println!("   - Targets: {:?}", t);
            println!("   - Extra packages: {}", if final_config.extra.is_empty() { "none".to_string() } else {
                final_config.extra.join(", ")
            });

            println!("\n{}{}Building project with Nix inside Docker...{}", BOLD, MAGENTA, RESET);

            let build_result = build_with_nix(&final_config.image, &final_config.project, &t, final_config.extra, &final_config.rust_channel, &final_config.rust_version, &final_config.nixpkgs_url).await;

            match build_result {
                Ok(_) => {
                    println!("\n{}{}Build completed successfully!{}", BOLD, GREEN, RESET);
                    let target_path = Path::new(&final_config.project).join("target/repx");

                    if target_path.exists() {
                        println!("{}{}Build artifacts are available in:{}", BOLD, CYAN, RESET);
                        println!("   - target/repx/ directory");
                    } else {
                        println!("\n{}{}WARNING:{} No build artifacts found in target/repx", BOLD, YELLOW, RESET);
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
                    eprintln!("   - Use --list-targets to see all available build targets");
                    Err(e)
                }
            }
        },
        Cli::Init { force } => {
            init_config(*force).await
        },
        Cli::Clean { project } => {
            clean_directories(project).await
        },
        Cli::Release => {
            print_version()
        }
    }
}
