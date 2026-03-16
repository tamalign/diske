use crate::scan::fs_tree::FsTree;
use crate::ui::colors::{color_for_extension, FileCategory};

/// Draw sidebar with top largest files and legend.
/// Returns Some(node_index) if user clicked an item to navigate to.
pub fn draw_sidebar(
    ui: &mut egui::Ui,
    tree: &FsTree,
    current_root: usize,
) -> Option<usize> {
    let mut navigate_to = None;

    let node = tree.get(current_root);
    ui.heading("diske");
    ui.separator();

    // Current directory info
    ui.label(format!("Total: {}", format_size(node.size)));
    ui.label(format!("Items: {}", count_descendants(tree, current_root)));
    ui.separator();

    // Top largest items in current view
    ui.strong("Largest Items");
    ui.add_space(4.0);

    let children = tree.children_of(current_root);
    let mut sorted: Vec<usize> = children.to_vec();
    sorted.sort_by(|&a, &b| tree.get(b).size.cmp(&tree.get(a).size));
    sorted.truncate(20);

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 120.0)
        .show(ui, |ui| {
            for &idx in &sorted {
                let item = tree.get(idx);
                if item.size == 0 {
                    continue;
                }
                let ext = tree.extension(idx);
                let color = color_for_extension(ext, item.is_dir);

                ui.horizontal(|ui| {
                    // Color indicator
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

                    if item.is_dir {
                        if ui.link(&label_text).clicked() {
                            navigate_to = Some(idx);
                        }
                    } else {
                        ui.label(&label_text);
                    }
                });
            }
        });

    ui.separator();

    // Legend
    ui.strong("Legend");
    ui.add_space(4.0);
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

    for cat in &categories {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(10.0, 10.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, cat.color());
            ui.label(cat.label());
        });
    }

    navigate_to
}

fn count_descendants(tree: &FsTree, index: usize) -> usize {
    let children = tree.children_of(index);
    let mut count = children.len();
    for &child in children {
        if tree.get(child).is_dir {
            count += count_descendants(tree, child);
        }
    }
    count
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
