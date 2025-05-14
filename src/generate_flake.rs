use std::path::Path;
use cargo_metadata::MetadataCommand;
use std::collections::HashMap;
use anyhow::{anyhow, Result};
use tera::Tera;
use crate::ExtraInput;
use crate::Templates;

/// Generate a flake.nix file based on project metadata
pub async fn generate_flake_file(flake_path: &Path, extra_inputs: &[ExtraInput]) -> Result<()> {
    // Get cargo metadata to extract real package name and version
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
    
    // Format extra inputs for the template
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
    
    // Load and render the flake template
    let template_content = Templates::get("flake.nix.hbs")
        .ok_or_else(|| anyhow!("Failed to find flake.nix.hbs template"))?;
    let template_str = std::str::from_utf8(template_content.data.as_ref())?;
    
    let mut tera = Tera::default();
    tera.add_raw_template("flake.nix.hbs", template_str)?;
    let rendered = tera.render("flake.nix.hbs", &context)?;
    
    // Write the rendered template to the flake.nix file
    tokio::fs::write(flake_path, rendered).await?;
    
    Ok(())
}