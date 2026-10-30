use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::Index;

pub const VERSION: u32 = 2;

pub fn cache_path() -> PathBuf {
    cache_dir().join("index.postcard")
}

pub fn cache_dir() -> PathBuf {
    if let Some(explicit) = env::var_os("NIX_CLEANER_CACHE") {
        return PathBuf::from(explicit);
    }
    let base = match env::var_os("XDG_CACHE_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home_dir().join(".cache"),
    };
    base.join("nix-cleaner")
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn save(index: &Index) -> Result<PathBuf> {
    let path = cache_path();
    save_to(&path, index)?;
    Ok(path)
}

pub fn save_to(path: &Path, index: &Index) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating cache directory {}", parent.display()))?;
    }
    let bytes = postcard::to_allocvec(index).context("serializing index")?;
    let staging = path.with_extension("postcard.tmp");
    fs::write(&staging, &bytes)
        .with_context(|| format!("writing {}", staging.display()))?;
    fs::rename(&staging, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub fn load() -> Result<Option<Index>> {
    load_from(&cache_path())
}

pub fn load_from(path: &Path) -> Result<Option<Index>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    let index: Index = postcard::from_bytes(&bytes)
        .with_context(|| format!("decoding {}", path.display()))?;
    if index.cache_version != VERSION {
        return Ok(None);
    }
    Ok(Some(index))
}
