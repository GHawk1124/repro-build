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
    
    let rendered = tera.render("flake.nix", &context)?;
    
    let mut file = File::create(flake_path).await?;
    file.write_all(rendered.as_bytes()).await?;
    
    Ok(rendered)
}