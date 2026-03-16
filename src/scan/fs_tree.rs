use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsNode {
    pub name: String,
    pub size: u64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub is_dir: bool,
    pub depth: u16,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsTree {
    pub nodes: Vec<FsNode>,
    pub root: usize,
}

impl FsTree {
    pub fn new(root_path: &Path) -> Self {
        let root_name = root_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root_path.to_string_lossy().to_string());

        let root_node = FsNode {
            name: root_name,
            size: 0,
            parent: None,
            children: Vec::new(),
            is_dir: true,
            depth: 0,
            path: root_path.to_path_buf(),
        };

        FsTree {
            nodes: vec![root_node],
            root: 0,
        }
    }

    pub fn add_node(
        &mut self,
        name: String,
        size: u64,
        parent: usize,
        is_dir: bool,
        path: PathBuf,
    ) -> usize {
        let depth = self.nodes[parent].depth + 1;
        let index = self.nodes.len();
        let node = FsNode {
            name,
            size,
            parent: Some(parent),
            children: Vec::new(),
            is_dir,
            depth,
            path,
        };
        self.nodes.push(node);
        self.nodes[parent].children.push(index);
        index
    }

    /// Propagate sizes from leaves up to root.
    pub fn compute_sizes(&mut self) {
        // Process nodes in reverse order (children before parents).
        for i in (0..self.nodes.len()).rev() {
            if self.nodes[i].is_dir {
                let child_sum: u64 = self.nodes[i]
                    .children
                    .iter()
                    .map(|&c| self.nodes[c].size)
                    .sum();
                self.nodes[i].size = child_sum;
            }
        }
    }

    /// Sort children of every directory by size descending.
    pub fn sort_children_by_size(&mut self) {
        for i in 0..self.nodes.len() {
            if self.nodes[i].is_dir {
                let nodes_ref = &self.nodes;
                let mut children = self.nodes[i].children.clone();
                children.sort_by(|&a, &b| nodes_ref[b].size.cmp(&nodes_ref[a].size));
                self.nodes[i].children = children;
            }
        }
    }

    pub fn get(&self, index: usize) -> &FsNode {
        &self.nodes[index]
    }

    pub fn children_of(&self, index: usize) -> &[usize] {
        &self.nodes[index].children
    }

    /// Build breadcrumb path from root to given node.
    pub fn ancestors(&self, index: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current = index;
        loop {
            path.push(current);
            match self.nodes[current].parent {
                Some(p) => current = p,
                None => break,
            }
        }
        path.reverse();
        path
    }

    /// Get total number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Get file extension for a node.
    pub fn extension(&self, index: usize) -> Option<&str> {
        if self.nodes[index].is_dir {
            return None;
        }
        self.nodes[index]
            .path
            .extension()
            .and_then(|e| e.to_str())
    }

    /// Remove a node and all its descendants. Returns the removed size.
    pub fn remove_node(&mut self, index: usize) -> u64 {
        let size = self.nodes[index].size;

        // Remove from parent's children list
        if let Some(parent) = self.nodes[index].parent {
            self.nodes[parent].children.retain(|&c| c != index);
        }

        // Mark node and descendants as empty (we don't actually remove from vec
        // to keep indices stable)
        self.mark_removed(index);

        // Recompute sizes up the ancestor chain
        self.compute_sizes();

        size
    }

    fn mark_removed(&mut self, index: usize) {
        let children: Vec<usize> = self.nodes[index].children.clone();
        for child in children {
            self.mark_removed(child);
        }
        self.nodes[index].size = 0;
        self.nodes[index].children.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tree() {
        let mut tree = FsTree::new(Path::new("/test"));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.get(0).name, "test");

        let dir_a = tree.add_node("dir_a".into(), 0, 0, true, PathBuf::from("/test/dir_a"));
        let file1 =
            tree.add_node("file1.txt".into(), 100, dir_a, false, PathBuf::from("/test/dir_a/file1.txt"));
        let _file2 =
            tree.add_node("file2.rs".into(), 200, dir_a, false, PathBuf::from("/test/dir_a/file2.rs"));
        let file3 =
            tree.add_node("file3.png".into(), 300, 0, false, PathBuf::from("/test/file3.png"));

        tree.compute_sizes();

        assert_eq!(tree.get(dir_a).size, 300); // 100 + 200
        assert_eq!(tree.get(0).size, 600); // 300 + 300
        assert_eq!(tree.get(file1).size, 100);
        assert_eq!(tree.get(file3).size, 300);
    }

    #[test]
    fn test_sort_children() {
        let mut tree = FsTree::new(Path::new("/test"));
        tree.add_node("small".into(), 10, 0, false, PathBuf::from("/test/small"));
        tree.add_node("big".into(), 1000, 0, false, PathBuf::from("/test/big"));
        tree.add_node("medium".into(), 100, 0, false, PathBuf::from("/test/medium"));

        tree.compute_sizes();
        tree.sort_children_by_size();

        let children = tree.children_of(0);
        assert_eq!(tree.get(children[0]).name, "big");
        assert_eq!(tree.get(children[1]).name, "medium");
        assert_eq!(tree.get(children[2]).name, "small");
    }

    #[test]
    fn test_ancestors() {
        let mut tree = FsTree::new(Path::new("/root"));
        let a = tree.add_node("a".into(), 0, 0, true, PathBuf::from("/root/a"));
        let b = tree.add_node("b".into(), 0, a, true, PathBuf::from("/root/a/b"));
        let c = tree.add_node("c.txt".into(), 50, b, false, PathBuf::from("/root/a/b/c.txt"));

        let ancestors = tree.ancestors(c);
        assert_eq!(ancestors, vec![0, a, b, c]);
    }

    #[test]
    fn test_remove_node() {
        let mut tree = FsTree::new(Path::new("/test"));
        let dir_a = tree.add_node("dir_a".into(), 0, 0, true, PathBuf::from("/test/dir_a"));
        tree.add_node("f1".into(), 100, dir_a, false, PathBuf::from("/test/dir_a/f1"));
        tree.add_node("f2".into(), 200, dir_a, false, PathBuf::from("/test/dir_a/f2"));
        let f3 = tree.add_node("f3".into(), 50, 0, false, PathBuf::from("/test/f3"));

        tree.compute_sizes();
        assert_eq!(tree.get(0).size, 350);

        tree.remove_node(dir_a);
        assert_eq!(tree.get(0).size, 50);
        assert_eq!(tree.get(f3).size, 50);
    }

    #[test]
    fn test_extension() {
        let mut tree = FsTree::new(Path::new("/test"));
        let f = tree.add_node("pic.png".into(), 100, 0, false, PathBuf::from("/test/pic.png"));
        let d = tree.add_node("dir".into(), 0, 0, true, PathBuf::from("/test/dir"));

        assert_eq!(tree.extension(f), Some("png"));
        assert_eq!(tree.extension(d), None);
    }
}
