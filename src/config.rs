use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepxConfig {
    /// Path location to your Cargo.toml or project root
    #[serde(default = "default_project")]
    pub project: String,
    
    /// Pin nix docker image to a specific version
    #[serde(default = "default_image")]
    pub image: String,
    
    /// Comma-separated list of targets to build for
    pub targets: Option<String>,
    
    /// Extra packages to install with nix
    #[serde(default)]
    pub extra: Vec<String>,
    
    /// Rust channel: stable or nightly
    #[serde(default = "default_rust_channel")]
    pub rust_channel: String,
    
    /// Rust version, e.g. '1.75.0' or 'latest'
    #[serde(default = "default_rust_version")]
    pub rust_version: String,
    
    /// nixpkgs URL/commit to use for reproducible builds
    #[serde(default = "default_nixpkgs_url")]
    pub nixpkgs_url: String,
}

fn default_project() -> String {
    ".".to_string()
}

fn default_image() -> String {
    "nixos/nix:latest".to_string()
}

fn default_rust_channel() -> String {
    "stable".to_string()
}

fn default_rust_version() -> String {
    "latest".to_string()
}

fn default_nixpkgs_url() -> String {
    "github:NixOS/nixpkgs/nixos-unstable".to_string()
}

impl Default for RepxConfig {
    fn default() -> Self {
        Self {
            project: default_project(),
            image: default_image(),
            targets: None,
            extra: Vec::new(),
            rust_channel: default_rust_channel(),
            rust_version: default_rust_version(),
            nixpkgs_url: default_nixpkgs_url(),
        }
    }
}

impl RepxConfig {
    /// Load configuration from a TOML file
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path).await?;
        let config: RepxConfig = toml::from_str(&content)?;
        Ok(config)
    }
    
    /// Save configuration to a TOML file
    pub async fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }
    
    /// Get the default config file path (repx.toml in current directory)
    pub fn default_config_path() -> &'static str {
        "repx.toml"
    }
    
    /// Check if a config file exists at the default location
    pub async fn config_exists() -> bool {
        fs::metadata(Self::default_config_path()).await.is_ok()
    }
} 