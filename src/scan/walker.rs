use std::collections::HashMap;
use std::path::Path;

use crossbeam_channel::Sender;
use jwalk::WalkDir;

use super::fs_tree::FsTree;

#[derive(Debug, Clone)]
pub enum ScanMessage {
    Progress {
        files_scanned: u64,
        bytes_scanned: u64,
        current_path: String,
    },
    /// Intermediate snapshot of the tree (sent periodically during scan)
    Snapshot(FsTree),
    Complete(FsTree),
    Error(String),
}

pub fn scan_directory(root: &Path, tx: Sender<ScanMessage>) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let result = do_scan(&root, &tx);
        match result {
            Ok(tree) => {
                let _ = tx.send(ScanMessage::Complete(tree));
            }
            Err(e) => {
                let _ = tx.send(ScanMessage::Error(e));
            }
        }
    });
}

fn do_scan(root: &Path, tx: &Sender<ScanMessage>) -> Result<FsTree, String> {
    let mut tree = FsTree::new(root);
    let mut path_to_index: HashMap<std::path::PathBuf, usize> = HashMap::new();
    path_to_index.insert(root.to_path_buf(), 0);

    let mut files_scanned: u64 = 0;
    let mut bytes_scanned: u64 = 0;

    let walker = WalkDir::new(root).skip_hidden(false).sort(true);

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        if path == root {
            continue;
        }

        let parent_path = match path.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };

        let parent_index = match path_to_index.get(&parent_path) {
            Some(&idx) => idx,
            None => continue,
        };

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let is_dir = entry.file_type().is_dir();
        let size = if is_dir {
            0
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };

        let index = tree.add_node(name, size, parent_index, is_dir, path.clone());

        if is_dir {
            path_to_index.insert(path.clone(), index);
        }

        files_scanned += 1;
        bytes_scanned += size;

        // Send progress every 500 files
        if files_scanned % 500 == 0 {
            let _ = tx.send(ScanMessage::Progress {
                files_scanned,
                bytes_scanned,
                current_path: path.to_string_lossy().to_string(),
            });
        }

        // Send intermediate snapshot every 5000 files
        if files_scanned % 5000 == 0 {
            let mut snapshot = tree.clone();
            snapshot.compute_sizes();
            snapshot.sort_children_by_size();
            let _ = tx.send(ScanMessage::Snapshot(snapshot));
        }
    }

    tree.compute_sizes();
    tree.sort_children_by_size();

    Ok(tree)
}
