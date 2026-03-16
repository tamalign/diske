use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Document,
    Executable,
    Other,
    Directory,
}

impl FileCategory {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "tiff" | "tif" | "ico"
            | "heic" | "heif" | "raw" | "cr2" | "nef" => FileCategory::Image,

            // Video
            "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
            | "3gp" => FileCategory::Video,

            // Audio
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "aiff" | "alac" => {
                FileCategory::Audio
            }

            // Archives
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "dmg" | "iso" | "pkg"
            | "deb" | "rpm" => FileCategory::Archive,

            // Code
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "java"
            | "go" | "rb" | "php" | "swift" | "kt" | "scala" | "sh" | "bash" | "zsh"
            | "fish" | "ps1" | "toml" | "yaml" | "yml" | "json" | "xml" | "html" | "css"
            | "scss" | "less" | "sql" | "md" | "rst" | "tex" | "vim" | "lua" | "r" | "m"
            | "el" | "lisp" | "clj" | "hs" | "ml" | "ex" | "exs" | "erl" | "zig" | "v"
            | "nim" | "cr" | "dart" | "wasm" => FileCategory::Code,

            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
            | "rtf" | "txt" | "csv" | "pages" | "numbers" | "key" => FileCategory::Document,

            // Executables
            "exe" | "app" | "dylib" | "so" | "dll" | "bin" | "msi" | "apk" | "ipa" => {
                FileCategory::Executable
            }

            _ => FileCategory::Other,
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            FileCategory::Image => Color32::from_rgb(66, 133, 244),     // Blue
            FileCategory::Video => Color32::from_rgb(52, 108, 204),     // Darker blue
            FileCategory::Audio => Color32::from_rgb(156, 100, 214),    // Purple
            FileCategory::Archive => Color32::from_rgb(234, 134, 46),   // Orange
            FileCategory::Code => Color32::from_rgb(76, 175, 100),      // Green
            FileCategory::Document => Color32::from_rgb(38, 166, 154),  // Teal
            FileCategory::Executable => Color32::from_rgb(214, 72, 72), // Red
            FileCategory::Other => Color32::from_rgb(158, 158, 158),    // Gray
            FileCategory::Directory => Color32::from_rgb(120, 144, 176),// Blue-gray
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FileCategory::Image => "Images",
            FileCategory::Video => "Video",
            FileCategory::Audio => "Audio",
            FileCategory::Archive => "Archives",
            FileCategory::Code => "Code",
            FileCategory::Document => "Documents",
            FileCategory::Executable => "Executables",
            FileCategory::Other => "Other",
            FileCategory::Directory => "Directories",
        }
    }
}

use crate::scan::fs_tree::FsTree;

/// Get the color for a node. Directories get a slightly darker shade of their dominant child category.
pub fn color_for_node(tree: &FsTree, index: usize) -> Color32 {
    let node = tree.get(index);
    if !node.is_dir {
        let ext = tree.extension(index);
        return match ext {
            Some(e) => FileCategory::from_extension(e).color(),
            None => FileCategory::Other.color(),
        };
    }

    // For directories, find the dominant category by bytes
    let dominant = dominant_category(tree, index);
    darken(dominant.color(), 0.15)
}

/// Get the color for a node given its extension (simple version).
pub fn color_for_extension(ext: Option<&str>, is_dir: bool) -> Color32 {
    if is_dir {
        return FileCategory::Directory.color();
    }
    match ext {
        Some(e) => FileCategory::from_extension(e).color(),
        None => FileCategory::Other.color(),
    }
}

/// Find the dominant file category in a directory by total bytes.
fn dominant_category(tree: &FsTree, index: usize) -> FileCategory {
    use std::collections::HashMap;
    let mut category_sizes: HashMap<FileCategory, u64> = HashMap::new();

    for &child in tree.children_of(index) {
        let child_node = tree.get(child);
        let cat = if child_node.is_dir {
            FileCategory::Directory
        } else {
            match tree.extension(child) {
                Some(e) => FileCategory::from_extension(e),
                None => FileCategory::Other,
            }
        };
        *category_sizes.entry(cat).or_insert(0) += child_node.size;
    }

    category_sizes
        .into_iter()
        .max_by_key(|&(_, size)| size)
        .map(|(cat, _)| cat)
        .unwrap_or(FileCategory::Directory)
}

/// Darken a color (for hover effect).
pub fn darken(color: Color32, amount: f32) -> Color32 {
    Color32::from_rgb(
        (color.r() as f32 * (1.0 - amount)) as u8,
        (color.g() as f32 * (1.0 - amount)) as u8,
        (color.b() as f32 * (1.0 - amount)) as u8,
    )
}

/// Lighten a color (for hover effect).
pub fn lighten(color: Color32, amount: f32) -> Color32 {
    Color32::from_rgb(
        (color.r() as f32 + (255.0 - color.r() as f32) * amount) as u8,
        (color.g() as f32 + (255.0 - color.g() as f32) * amount) as u8,
        (color.b() as f32 + (255.0 - color.b() as f32) * amount) as u8,
    )
}
