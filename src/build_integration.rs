use anyhow::Result;
use std::env;
use std::path::{Path, PathBuf};

/// Helper function to be called from a build.rs script to perform a reproducible build
///
/// # Example
/// ```no_run
/// // In build.rs
/// #[tokio::main]
/// async fn main() {
///     if let Err(e) = repx_lib::build_script::run_build().await {
///         eprintln!("Reproducible build failed: {}", e);
///         std::process::exit(1);
///     }
/// }
/// ```
pub async fn run_build() -> Result<()> {
    // Get environment variables that Cargo sets for build scripts
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let _package_name = env::var("CARGO_PKG_NAME").expect("CARGO_PKG_NAME not set");

    // Use defaults that can be overridden via environment variables
    let nix_image = env::var("REPRO_BUILD_IMAGE").unwrap_or_else(|_| "nixos/nix:latest".to_string());
    let targets = env::var("REPRO_BUILD_TARGETS").unwrap_or_else(|_| "x86_64-linux-gnu".to_string());
    let rust_channel = env::var("REPRO_BUILD_RUST_CHANNEL").unwrap_or_else(|_| "stable".to_string());
    let rust_version = env::var("REPRO_BUILD_RUST_VERSION").unwrap_or_else(|_| "latest".to_string());
    let nixpkgs_url = env::var("REPRO_BUILD_NIXPKGS_URL").unwrap_or_else(|_| "github:NixOS/nixpkgs/nixos-unstable".to_string());

    // Parse extra packages from environment variables
    let extra_packages = parse_extra_packages_from_env();

    // Parse targets into a vector
    let targets_vec: Vec<&str> = targets.split(',').collect();

    // Run the build
    println!("cargo:warning=Starting reproducible build with Nix inside Docker...");
    println!("cargo:warning=Project: {}", cargo_manifest_dir);
    println!("cargo:warning=Docker Image: {}", nix_image);
    println!("cargo:warning=Targets: {:?}", targets_vec);

    // Call the main build function
    let result = crate::build_with_nix(
        &nix_image,
        &cargo_manifest_dir,
        &targets_vec,
        extra_packages,
        &rust_channel,
        &rust_version,
        &nixpkgs_url,
    ).await;

    // Handle the result
    match result {
        Ok(_) => {
            let target_path = Path::new(&cargo_manifest_dir).join("target/repx");
            println!("cargo:warning=Build completed successfully!");

            if target_path.exists() {
                println!("cargo:warning=Build artifacts available in target/repx/");

                // Copy artifacts to OUT_DIR if requested
                if env::var("REPX_COPY_TO_OUT_DIR").unwrap_or_else(|_| "false".to_string()) == "true" {
                    copy_artifacts_to_out_dir(&target_path, &PathBuf::from(out_dir)).await?;
                    println!("cargo:warning=Artifacts copied to OUT_DIR");
                }
            }
            Ok(())
        },
        Err(e) => {
            println!("cargo:warning=Build failed: {}", e);
            Err(e)
        }
    }
}

/// Parse extra packages from environment variables
/// Format: REPRO_BUILD_EXTRA_PACKAGE_1=openssl, REPRO_BUILD_EXTRA_PACKAGE_2=pkg-config, etc.
/// Or: REPRO_BUILD_EXTRA_PACKAGES=openssl,pkg-config,curl
fn parse_extra_packages_from_env() -> Vec<String> {
    let mut extra_packages = Vec::new();

    // First try the comma-separated list format
    if let Ok(packages_str) = env::var("REPRO_BUILD_EXTRA_PACKAGES") {
        extra_packages.extend(
            packages_str.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        );
    }

    // Then try individual package environment variables
    for (key, value) in env::vars() {
        if key.starts_with("REPRO_BUILD_EXTRA_PACKAGE_") {
            let package = value.trim().to_string();
            if !package.is_empty() {
                extra_packages.push(package);
            }
        }
    }

    extra_packages
}

/// Copy build artifacts to OUT_DIR
async fn copy_artifacts_to_out_dir(source_dir: &Path, out_dir: &Path) -> Result<()> {
    use tokio::fs;

    let mut entries = fs::read_dir(source_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let target_path = out_dir.join(&file_name);

        if path.is_dir() {
            fs::create_dir_all(&target_path).await?;
            // Use Box::pin to handle the recursive async call
            Box::pin(copy_artifacts_to_out_dir(&path, &target_path)).await?;
        } else {
            fs::copy(&path, &target_path).await?;
        }
    }

    Ok(())
}