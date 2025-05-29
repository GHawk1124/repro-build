use std::path::Path;
use cargo_metadata::MetadataCommand;
use anyhow::Result;
use tera::Tera;
use crate::ExtraInput;
use crate::FLAKE_TEMPLATE;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tera::Context;

/// Generate a flake.nix file for the Rust project
pub async fn generate_flake_file(
    flake_path: &Path,
    extra_inputs: &[ExtraInput],
    rust_channel: &str,
    rust_version: &str,
    nixpkgs_url: &str,
) -> Result<String> {
    let metadata = MetadataCommand::new().exec()?;
    let package = metadata.packages
        .iter()
        .find(|p| p.id.repr == metadata.workspace_members[0].repr)
        .ok_or_else(|| anyhow::anyhow!("Could not find package in metadata"))?;

    let mut tera = Tera::default();
    tera.add_raw_template("flake.nix", FLAKE_TEMPLATE)?;

    let mut context = Context::new();
    context.insert("package_name", &package.name);
    context.insert("package_version", &package.version.to_string());
    context.insert("extra_inputs", &extra_inputs);
    context.insert("rust_channel", rust_channel);
    context.insert("rust_version", rust_version);
    context.insert("nixpkgs_url", nixpkgs_url);
    context.insert("musl_version", "");

    let rendered = tera.render("flake.nix", &context)?;

    // Normalize line endings to Unix-style (LF only) to ensure compatibility with Nix in Linux containers
    let normalized_content = rendered.replace("\r\n", "\n").replace("\r", "\n");

    let mut file = File::create(flake_path).await?;
    file.write_all(normalized_content.as_bytes()).await?;

    Ok(normalized_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_flake_generation_without_musl_override() {
        // Test that flake generation works without musl version override
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let extra_inputs = vec![];
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let content = rt.block_on(async {
            generate_flake_file(
                temp_file.path(),
                &extra_inputs,
                "stable",
                "latest",
                "github:NixOS/nixpkgs/nixos-unstable",
            ).await.unwrap()
        });

        // Check that no musl overlay is included
        assert!(!content.contains("musl = prev.musl.overrideAttrs"));
        // But musl targets should still be supported
        assert!(content.contains("x86_64-linux-musl"));
        assert!(content.contains("aarch64-linux-musl"));
    }
}