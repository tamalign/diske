use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::fs_tree::FsTree;

const CACHE_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct CacheEnvelope {
    version: u32,
    tree: FsTree,
}

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

    let envelope = CacheEnvelope {
        version: CACHE_VERSION,
        tree: tree.clone(),
    };
    let json =
        serde_json::to_vec(&envelope).map_err(|e| format!("Serialization failed: {}", e))?;
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

    // Try new versioned format first
    if let Ok(envelope) = serde_json::from_slice::<CacheEnvelope>(&data) {
        if envelope.version == CACHE_VERSION {
            return Some(envelope.tree);
        }
        // Version mismatch — discard stale cache
        let _ = fs::remove_file(&cache_file);
        return None;
    }

    // Incompatible format — discard
    let _ = fs::remove_file(&cache_file);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::fs_tree::FsTree;

    #[test]
    fn test_roundtrip_serialization() {
        let mut tree = FsTree::new(Path::new("/tmp/test_diske_cache"));
        tree.add_node("dir_a".into(), 0, 0, true, PathBuf::from("/tmp/test_diske_cache/dir_a"));
        tree.add_node("file1.txt".into(), 100, 1, false, PathBuf::from("/tmp/test_diske_cache/dir_a/file1.txt"));
        tree.add_node("file2.rs".into(), 200, 0, false, PathBuf::from("/tmp/test_diske_cache/file2.rs"));
        tree.compute_sizes();
        tree.sort_children_by_size();

        let envelope = CacheEnvelope {
            version: CACHE_VERSION,
            tree: tree.clone(),
        };
        let json = serde_json::to_vec(&envelope).unwrap();
        let loaded: CacheEnvelope = serde_json::from_slice(&json).unwrap();

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.tree.len(), tree.len());
        assert_eq!(loaded.tree.get(loaded.tree.root).size, tree.get(tree.root).size);
        assert_eq!(loaded.tree.get(1).name, "dir_a");
        assert_eq!(loaded.tree.get(2).name, "file1.txt");
        assert_eq!(loaded.tree.get(2).size, 100);
        assert_eq!(loaded.tree.get(0).descendant_count, 3);
    }

    #[test]
    fn test_old_format_rejected() {
        // Simulate an old-format cache (just a bare FsTree, no envelope)
        let tree = FsTree::new(Path::new("/tmp/test_old"));
        let bare_json = serde_json::to_vec(&tree).unwrap();

        // Trying to parse as CacheEnvelope should fail (no "version" field)
        let result = serde_json::from_slice::<CacheEnvelope>(&bare_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_simple_hash_deterministic() {
        assert_eq!(simple_hash("/home/user"), simple_hash("/home/user"));
        assert_ne!(simple_hash("/home/user"), simple_hash("/home/other"));
    }
}
