use super::{metadata, pacman};
use crate::{
    error::{AppError, Result},
    packages::Catalog,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    fingerprint: u64,
    catalog: Catalog,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".cache"))
        })?;
    Some(base.join("pacstore/catalog-v1.json"))
}

fn read(path: &Path) -> Option<Cache> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Cache>(&bytes) {
            Ok(cache) if cache.version == 2 => Some(cache),
            _ => {
                log::warn!("Ignoring invalid or unsupported catalog cache");
                None
            }
        },
        Err(_) => None,
    }
}

fn stamp(path: &Path, hash: &mut DefaultHasher) {
    path.hash(hash);
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(hash);
        metadata.modified().ok().hash(hash);
    }
}

fn fingerprint(configuration: &str, db: &Path, metadata_files: &[PathBuf]) -> u64 {
    let mut hash = DefaultHasher::new();
    configuration.hash(&mut hash);
    stamp(Path::new("/usr/bin/pacman"), &mut hash);
    for directory in [db.join("local"), db.join("sync")] {
        stamp(&directory, &mut hash);
        if let Ok(entries) = fs::read_dir(&directory) {
            let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                stamp(&path, &mut hash);
                if path.is_dir() {
                    stamp(&path.join("desc"), &mut hash);
                }
            }
        }
    }
    for file in metadata_files {
        stamp(file, &mut hash);
    }
    hash.finish()
}

fn write(path: &Path, cache: &Cache) -> Result<()> {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).map_err(|source| AppError::Io {
        operation: "Create catalog cache directory",
        source,
    })?;
    let bytes = serde_json::to_vec(cache)
        .map_err(|e| AppError::Backend(format!("Encode catalog cache: {e}")))?;
    let temporary = parent.join(format!(".catalog-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| AppError::Io {
            operation: "Create temporary catalog cache",
            source,
        })?;
    let result = (|| {
        use std::io::Write;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|source| AppError::Io {
        operation: "Save catalog cache",
        source,
    })
}

fn stale(mut cache: Cache, error: AppError) -> Catalog {
    cache.catalog.source = "Offline catalog snapshot · Current package state unavailable".into();
    cache.catalog.updates_known = false;
    cache.catalog.warnings.push(format!("{error}. Showing the last catalog snapshot; installed state may be stale. Reload when pacman is available."));
    for p in &mut cache.catalog.packages {
        p.update_available = false;
    }
    cache.catalog
}

pub fn load(force: bool) -> Result<Catalog> {
    let location = path();
    let cached = location.as_deref().and_then(read);
    let fresh = (|| {
        let (configuration, dbpath, repositories) = pacman::configuration()?;
        let root = configuration
            .lines()
            .find_map(|line| line.strip_prefix("RootDir = "))
            .unwrap_or("/");
        let root = Path::new(root);
        let db = Path::new(&dbpath);
        let inputs = metadata::inputs(root);
        let before = fingerprint(&configuration, db, &inputs);
        if !force
            && let Some(cache) = &cached
            && cache.fingerprint == before
        {
            let mut catalog = cache.catalog.clone();
            catalog.source = "Catalog cache · Validated against local pacman databases".into();
            return Ok(catalog);
        }
        let mut catalog = pacman::query(repositories)?;
        let ownership = match pacman::ownership() {
            Ok(ownership) => ownership,
            Err(error) => {
                catalog
                    .warnings
                    .push(format!("Application ownership unavailable: {error}"));
                Default::default()
            }
        };
        metadata::enrich(&mut catalog, root, &inputs, &ownership);
        let after_inputs = metadata::inputs(root);
        if before != fingerprint(&configuration, db, &after_inputs) {
            return Err(AppError::Backend("Package databases or application metadata changed while reading. Reload to obtain a consistent catalog.".into()));
        }
        if let Some(location) = &location {
            let cache = Cache {
                version: 2,
                fingerprint: before,
                catalog: catalog.clone(),
            };
            if let Err(error) = write(location, &cache) {
                catalog.warnings.push(error.to_string());
            }
        }
        Ok(catalog)
    })();
    match fresh {
        Ok(catalog) => Ok(catalog),
        Err(error) => match cached {
            Some(cache) => Ok(stale(cache, error)),
            None => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_fallback_never_claims_current_update_state() {
        let catalog = stale(
            Cache {
                version: 2,
                fingerprint: 0,
                catalog: Catalog::demo(),
            },
            AppError::Backend("offline".into()),
        );
        assert!(!catalog.updates_known);
        assert!(!catalog.packages.iter().any(|p| p.has_update()));
        assert!(!catalog.warnings.is_empty());
    }
    #[test]
    fn catalog_round_trip_preserves_metadata() {
        let cache = Cache {
            version: 2,
            fingerprint: 42,
            catalog: Catalog::demo(),
        };
        let bytes = serde_json::to_vec(&cache).unwrap();
        let restored: Cache = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.fingerprint, 42);
        assert_eq!(restored.catalog.packages[0].name, "firefox");
    }
}
