use std::path::Path;

use crate::scan::fs_tree::FsTree;
use crate::ui::colors::{color_for_extension, FileCategory};

const TOP_ITEMS_COUNT: usize = 20;
const SIDEBAR_SCROLL_BOTTOM_MARGIN: f32 = 120.0;

/// Disk volume info
struct VolumeInfo {
    name: String,
    total: u64,
    available: u64,
}

#[cfg(unix)]
fn get_volume_info(path: &Path) -> Option<VolumeInfo> {
    use std::ffi::CString;
    let path_cstr = CString::new(path.to_string_lossy().as_bytes()).ok()?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path_cstr.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let block_size = stat.f_frsize as u64;
        let total = stat.f_blocks as u64 * block_size;
        let available = stat.f_bavail as u64 * block_size;

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        Some(VolumeInfo {
            name,
            total,
            available,
        })
    }
}

#[cfg(windows)]
fn get_volume_info(path: &Path) -> Option<VolumeInfo> {
    use std::os::windows::ffi::OsStrExt;
    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut _total_free_bytes: u64 = 0;

    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path_wide.as_ptr(),
            &mut free_bytes_available,
            &mut total_bytes,
            &mut _total_free_bytes,
        )
    };
    if ok == 0 {
        return None;
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    Some(VolumeInfo {
        name,
        total: total_bytes,
        available: free_bytes_available,
    })
}

#[cfg(unix)]
fn get_all_volumes() -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();

    // Root volume
    if let Some(info) = get_volume_info(Path::new("/")) {
        volumes.push(VolumeInfo {
            name: "Macintosh HD".to_string(),
            ..info
        });
    }

    // External/additional volumes under /Volumes
    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip symlinks to root
                if let Ok(target) = std::fs::read_link(&path) {
                    if target == Path::new("/") {
                        continue;
                    }
                }
                if let Some(info) = get_volume_info(&path) {
                    // Skip if same as root (some /Volumes entries point to root)
                    if volumes.first().map(|v| v.total) == Some(info.total)
                        && volumes.first().map(|v| v.available) == Some(info.available)
                    {
                        continue;
                    }
                    volumes.push(info);
                }
            }
        }
    }

    volumes
}

#[cfg(windows)]
fn get_all_volumes() -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    // Check drive letters A: through Z:
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let path = Path::new(&drive);
        if path.exists() {
            if let Some(info) = get_volume_info(path) {
                let name = if info.name.is_empty() || info.name == "\\" {
                    format!("Drive ({}:)", letter as char)
                } else {
                    info.name
                };
                volumes.push(VolumeInfo { name, ..info });
            }
        }
    }
    volumes
}

/// Compute category sizes for a given root. Call this when tree/root changes, not every frame.
pub fn compute_category_sizes(tree: &FsTree, root: usize) -> Vec<(FileCategory, u64)> {
    compute_category_sizes_impl(tree, root)
}

/// Draw sidebar with storage info, top largest files, and legend.
/// Returns Some(node_index) if user clicked an item to navigate to.
/// When `search_results` is non-empty, shows search results instead of largest items.
pub fn draw_sidebar(
    ui: &mut egui::Ui,
    tree: &FsTree,
    current_root: usize,
    search_results: &[usize],
    category_sizes: &[(FileCategory, u64)],
) -> Option<usize> {
    let mut navigate_to = None;

    let node = tree.get(current_root);
    ui.heading("diske");
    ui.separator();

    // Storage volumes
    ui.strong("Storage");
    ui.add_space(4.0);
    let volumes = get_all_volumes();
    for vol in &volumes {
        let used = vol.total.saturating_sub(vol.available);
        let usage_ratio = if vol.total > 0 {
            used as f32 / vol.total as f32
        } else {
            0.0
        };

        ui.label(format!("{}", vol.name));
        ui.horizontal(|ui| {
            let bar_width = ui.available_width() - 4.0;
            let bar_height = 14.0;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());

            // Background
            ui.painter().rect_filled(
                rect,
                3.0,
                egui::Color32::from_rgb(60, 60, 60),
            );

            // Used portion
            let used_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(bar_width * usage_ratio, bar_height),
            );
            let bar_color = if usage_ratio > 0.9 {
                egui::Color32::from_rgb(214, 72, 72) // Red when almost full
            } else if usage_ratio > 0.75 {
                egui::Color32::from_rgb(234, 180, 46) // Yellow when getting full
            } else {
                egui::Color32::from_rgb(66, 133, 244) // Blue normal
            };
            ui.painter().rect_filled(used_rect, 3.0, bar_color);
        });
        ui.label(format!(
            "{} / {} ({} free)",
            format_size(used),
            format_size(vol.total),
            format_size(vol.available),
        ));
        ui.add_space(4.0);
    }
    ui.separator();

    // Current directory info
    ui.label(format!("Current: {}", format_size(node.size)));
    ui.label(format!("Items: {}", node.descendant_count));
    ui.separator();

    // Show search results or top largest items
    let items_to_show: Vec<usize>;
    if !search_results.is_empty() {
        ui.strong(format!("Search Results ({})", search_results.len()));
        ui.add_space(4.0);
        items_to_show = search_results.to_vec();
    } else {
        ui.strong("Largest Items");
        ui.add_space(4.0);
        let children = tree.children_of(current_root);
        let mut sorted: Vec<usize> = children.to_vec();
        sorted.sort_by(|&a, &b| tree.get(b).size.cmp(&tree.get(a).size));
        sorted.truncate(TOP_ITEMS_COUNT);
        items_to_show = sorted;
    }

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - SIDEBAR_SCROLL_BOTTOM_MARGIN)
        .show(ui, |ui| {
            for &idx in &items_to_show {
                let item = tree.get(idx);
                if item.size == 0 {
                    continue;
                }
                let ext = tree.extension(idx);
                let color = color_for_extension(ext, item.is_dir);

                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(10.0, 10.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 2.0, color);

                    let label_text = format!(
                        "{} ({})",
                        item.name,
                        format_size(item.size)
                    );

                    // In search results, directories are clickable to navigate
                    if item.is_dir {
                        if ui.link(&label_text).clicked() {
                            navigate_to = Some(idx);
                        }
                    } else {
                        // Files in search results: click to navigate to parent
                        if !search_results.is_empty() {
                            if let Some(parent) = item.parent {
                                if ui.link(&label_text).clicked() {
                                    navigate_to = Some(parent);
                                }
                            } else {
                                ui.label(&label_text);
                            }
                        } else {
                            ui.label(&label_text);
                        }
                    }
                });
            }
        });

    ui.separator();

    // Category breakdown — shows size per file type with bars
    ui.strong("By Type");
    ui.add_space(4.0);

    let max_size = category_sizes.iter().map(|&(_, s)| s).max().unwrap_or(1);

    for &(cat, size) in category_sizes {
        if size == 0 {
            continue;
        }
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(10.0, 10.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, cat.color());

            let bar_width = (ui.available_width() - 4.0).max(0.0);
            let bar_height = 10.0;
            let ratio = size as f32 / max_size as f32;

            let (bar_rect, _) =
                ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());
            // Background
            ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(50, 50, 50));
            // Fill
            let fill_rect = egui::Rect::from_min_size(
                bar_rect.min,
                egui::vec2(bar_width * ratio, bar_height),
            );
            ui.painter().rect_filled(fill_rect, 2.0, cat.color());
        });
        ui.label(format!("{}: {}", cat.label(), format_size(size)));
    }

    navigate_to
}

fn compute_category_sizes_impl(tree: &FsTree, root: usize) -> Vec<(FileCategory, u64)> {
    let categories = [
        FileCategory::Image,
        FileCategory::Video,
        FileCategory::Audio,
        FileCategory::Archive,
        FileCategory::Code,
        FileCategory::Document,
        FileCategory::Executable,
        FileCategory::Other,
    ];
    let mut sizes = [0u64; 8];

    let mut stack: Vec<usize> = tree.children_of(root).to_vec();
    while let Some(idx) = stack.pop() {
        let node = tree.get(idx);
        if node.is_dir {
            for &child in tree.children_of(idx) {
                stack.push(child);
            }
        } else {
            let cat = match tree.extension(idx) {
                Some(e) => FileCategory::from_extension(e),
                None => FileCategory::Other,
            };
            sizes[cat as usize] += node.size;
        }
    }

    let mut result: Vec<(FileCategory, u64)> = categories
        .iter()
        .enumerate()
        .map(|(i, &cat)| (cat, sizes[i]))
        .collect();
    result.sort_by(|a, b| b.1.cmp(&a.1));
    result
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
