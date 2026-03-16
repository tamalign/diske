use std::fs;
use std::path::{Path, PathBuf};

use super::fs_tree::FsTree;

fn cache_dir() -> Option<PathBuf> {
    dirs_next::cache_dir().map(|d| d.join("diske"))
}

fn cache_file_for(scan_root: &Path) -> Option<PathBuf> {
    let dir = cache_dir()?;
    // Use a hash of the path as filename to avoid path separator issues
    let hash = simple_hash(scan_root.to_string_lossy().as_ref());
    Some(dir.join(format!("{}.json", hash)))
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

/// Save a scan result to disk cache.
pub fn save(tree: &FsTree) -> Result<(), String> {
    let root_path = &tree.nodes[tree.root].path;
    let cache_file = cache_file_for(root_path).ok_or("Cannot determine cache directory")?;

    if let Some(parent) = cache_file.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create cache dir: {}", e))?;
    }

    let json = serde_json::to_vec(tree).map_err(|e| format!("Serialization failed: {}", e))?;
    fs::write(&cache_file, json).map_err(|e| format!("Failed to write cache: {}", e))?;

    Ok(())
}

/// Load a cached scan result if available.
pub fn load(scan_root: &Path) -> Option<FsTree> {
    let cache_file = cache_file_for(scan_root)?;

    if !cache_file.exists() {
        return None;
    }

    let data = fs::read(&cache_file).ok()?;
    serde_json::from_slice(&data).ok()
}
