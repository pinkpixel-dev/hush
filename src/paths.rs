use std::path::PathBuf;

use anyhow::{Context, Result};

/// Where hush keeps its vault and config. `HUSH_DIR` overrides the default
/// (`~/.local/share/hush` on Linux).
pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        if let Some(dir) = std::env::var_os("HUSH_DIR") {
            let dir = std::path::absolute(PathBuf::from(dir)).context("HUSH_DIR is not a usable path")?;
            return Ok(Self { dir });
        }
        let dirs = directories::ProjectDirs::from("dev", "pinkpixel", "hush")
            .context("couldn't find a home directory for the current user")?;
        Ok(Self { dir: dirs.data_dir().to_path_buf() })
    }

    pub fn vault(&self) -> PathBuf {
        self.dir.join("vault.age")
    }

    pub fn config(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    pub fn lock(&self) -> PathBuf {
        self.dir.join("vault.lock")
    }
}
