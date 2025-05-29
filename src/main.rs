use anyhow::Result;
use clap::Parser;
use cargo_metadata::MetadataCommand;
use repx_lib::{build_with_nix, ExtraInput, RESET, BOLD, GREEN, RED, YELLOW, CYAN, MAGENTA};
use std::path::Path;

#[derive(Parser)]
#[command(name = "repx", about = "Cargo subcommand for Nix-based Rust builds")]
enum Cli {
    Build {
        #[arg(short, long, default_value = ".", help = "Path location to your Cargo.toml or project root.")]
        project: String,
        #[arg(short, long, default_value = "nixos/nix:latest", help = "Pin nix docker image to a specific version.")]
        image: String,
        #[arg(short, long, help = "Comma-separated list of targets to build for. If not specified, builds for host target.")]
        targets: Option<String>,
        #[arg(long, help = "List all available targets and exit")]
        list_targets: bool,
        #[arg(long, value_delimiter = ',', help = "Extra packages to install with nix.")]
        extra: Vec<String>,
        #[arg(long, default_value = "stable", help = "Rust channel: stable or nightly")]
        rust_channel: String,
        #[arg(long, default_value = "latest", help = "Rust version, e.g. '1.75.0' or 'latest'")]
        rust_version: String,
        #[arg(long, default_value = "github:NixOS/nixpkgs/nixos-unstable", help = "nixpkgs URL/commit to use for reproducible builds")]
        nixpkgs_url: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse() {
        Cli::Build { project, image, targets, list_targets, extra, rust_channel, rust_version, nixpkgs_url } => {
            if list_targets {
                print_available_targets();
                return Ok(());
            }

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

            // Determine targets to build
            let target_string = match targets {
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
            println!("   - Project: {}", project);
            println!("   - Docker Image: {}", image);
            println!("   - Rust: {} {}", rust_channel, rust_version);
            println!("   - nixpkgs: {}", nixpkgs_url);
            println!("   - Targets: {:?}", t);
            println!("   - Extra inputs: {}", if extra_inputs.is_empty() { "none".to_string() } else {
                extra_inputs.iter().map(|i| format!("{}={}", i.name, i.url)).collect::<Vec<_>>().join(", ")
            });

            println!("\n{}{}Building project with Nix inside Docker...{}", BOLD, MAGENTA, RESET);

            let build_result = build_with_nix(&image, &project, &t, extra_inputs, &rust_channel, &rust_version, &nixpkgs_url).await;

            match build_result {
                Ok(_) => {
                    println!("\n{}{}Build completed successfully!{}", BOLD, GREEN, RESET);
                    let target_path = Path::new(&project).join("target/repx");

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
        Cli::Release => {
            print_version()?;
            Ok(())
        }
    }
}
