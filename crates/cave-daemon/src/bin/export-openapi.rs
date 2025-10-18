use std::{env, fs, path::Path};

use anyhow::{Context, Result};
use cave_daemon::server::docs::ApiDoc;

fn main() -> Result<()> {
    let output = env::args().nth(1);
    let openapi = ApiDoc::openapi();
    let yaml = openapi.to_yaml().context("failed to serialise OpenAPI")?;

    if let Some(path) = output {
        let path = Path::new(&path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create directory for {}", path.display())
                })?;
            }
        }
        fs::write(path, yaml).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{yaml}");
    }

    Ok(())
}
