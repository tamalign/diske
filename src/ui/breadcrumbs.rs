use crate::scan::fs_tree::FsTree;

/// Draw breadcrumb navigation bar. Returns Some(index) if user clicked a breadcrumb.
pub fn draw_breadcrumbs(ui: &mut egui::Ui, tree: &FsTree, current_root: usize) -> Option<usize> {
    let ancestors = tree.ancestors(current_root);
    let mut clicked = None;

    ui.horizontal(|ui| {
        for (i, &idx) in ancestors.iter().enumerate() {
            let node = tree.get(idx);
            let is_last = i == ancestors.len() - 1;

            if is_last {
                ui.strong(&node.name);
            } else {
                if ui.link(&node.name).clicked() {
                    clicked = Some(idx);
                }
                ui.label("/");
            }
        }
    });

    clicked
}
