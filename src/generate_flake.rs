use std::path::Path;
use cargo_metadata::MetadataCommand;
use std::collections::HashMap;
use anyhow::{anyhow, Result};
use tera::Tera;
use crate::ExtraInput;
use crate::FLAKE_TEMPLATE;

/// Generate a flake.nix file based on project metadata
pub async fn generate_flake_file(flake_path: &Path, extra_inputs: &[ExtraInput], rust_channel: &str, rust_version: &str) -> Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()?;
    let package = metadata.packages.first()
        .ok_or_else(|| anyhow!("No packages found in cargo metadata"))?;
    let package_name = &package.name;
    let package_version = &package.version.to_string();
    let mut context = tera::Context::new();
    context.insert("package_name", package_name);
    context.insert("package_version", package_version);
    context.insert("rust_channel", rust_channel);
    context.insert("rust_version", rust_version);
    let extra_inputs_formatted: Vec<HashMap<String, String>> = extra_inputs
        .iter()
        .map(|input| {
            let mut map = HashMap::new();
            map.insert("name".to_string(), input.name.clone());
            map.insert("url".to_string(), input.url.clone());
            map
        })
        .collect();
    context.insert("extra_inputs", &extra_inputs_formatted);
    let mut tera = Tera::default();
    tera.add_raw_template("flake.nix.hbs", FLAKE_TEMPLATE)?;
    let rendered = tera.render("flake.nix.hbs", &context)?;
    tokio::fs::write(flake_path, rendered).await?;
    Ok(())
}