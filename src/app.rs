use crossbeam_channel::{Receiver, unbounded};
use egui::Vec2;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::scan::cache;
use crate::scan::fs_tree::FsTree;

const TOAST_DURATION_SECS: f32 = 3.0;
const TOAST_FADE_START_SECS: f32 = 2.0;
const SIDEBAR_DEFAULT_WIDTH: f32 = 220.0;
const SEARCH_MAX_RESULTS: usize = 100;
use crate::scan::walker::{ScanMessage, scan_directory};
use crate::treemap::layout::LayoutRect;
use crate::ui::colors::FileCategory;
use crate::ui::{breadcrumbs, sidebar, status_bar, treemap_view};

#[derive(PartialEq)]
enum AppState {
    Welcome,
    Scanning,
    Viewing,
    Error(String),
}

enum ContextAction {
    Reveal(PathBuf),
    CopyPath(String),
    ConfirmTrash(PathBuf, String, usize),
    None,
}

pub struct DiskApp {
    state: AppState,
    tree: Option<FsTree>,
    current_root: usize,
    navigation_history: Vec<usize>,

    // Scan state
    scan_receiver: Option<Receiver<ScanMessage>>,
    scan_root: Option<PathBuf>,
    files_scanned: u64,
    bytes_scanned: u64,
    current_scan_path: String,

    // Layout cache
    layout_cache: Option<(usize, Vec2, Vec<LayoutRect>)>,

    // Context menu
    context_menu_node: Option<usize>,
    trash_confirm: Option<(PathBuf, String, usize)>, // (path, name, node_idx)

    // Paths trashed during current scan (to filter from incoming scan results)
    trashed_paths: HashSet<PathBuf>,

    // Search
    search_query: String,
    search_results: Vec<usize>,

    // Category size cache (root, data)
    category_cache: Option<(usize, Vec<(FileCategory, u64)>)>,

    // Toast message
    toast_message: Option<(String, std::time::Instant)>,
}

// --- Initialization and scan management ---

impl DiskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // Load Japanese-capable font
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) =
            std::fs::read("/System/Library/Fonts/Supplemental/Arial Unicode.ttf")
        {
            fonts.font_data.insert(
                "arial_unicode".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("arial_unicode".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("arial_unicode".to_owned());
        }
        cc.egui_ctx.set_fonts(fonts);

        let mut app = Self {
            state: AppState::Welcome,
            tree: None,
            current_root: 0,
            navigation_history: Vec::new(),
            scan_receiver: None,
            scan_root: None,
            files_scanned: 0,
            bytes_scanned: 0,
            current_scan_path: String::new(),
            layout_cache: None,
            context_menu_node: None,
            trash_confirm: None,
            trashed_paths: HashSet::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            category_cache: None,
            toast_message: None,
        };

        // Try to load cache first, then start background rescan
        if let Some(home) = dirs_next::home_dir() {
            if let Some(cached_tree) = cache::load(&home) {
                app.files_scanned = cached_tree.len() as u64;
                app.bytes_scanned = cached_tree.get(cached_tree.root).size;
                app.tree = Some(cached_tree);
                app.state = AppState::Viewing;
            }
            app.start_scan_preserving_view(home);
        }

        app
    }

    fn start_scan(&mut self, path: PathBuf) {
        let (tx, rx) = unbounded();
        self.scan_receiver = Some(rx);
        self.scan_root = Some(path.clone());
        self.state = AppState::Scanning;
        self.files_scanned = 0;
        self.bytes_scanned = 0;
        self.current_scan_path.clear();
        self.tree = None;
        self.layout_cache = None;
        self.category_cache = None;
        self.navigation_history.clear();
        self.current_root = 0;
        self.trashed_paths.clear();

        scan_directory(&path, tx);
    }

    fn start_scan_preserving_view(&mut self, path: PathBuf) {
        let (tx, rx) = unbounded();
        self.scan_receiver = Some(rx);
        self.scan_root = Some(path.clone());
        if self.tree.is_none() {
            self.state = AppState::Scanning;
        }
        self.files_scanned = 0;
        self.bytes_scanned = 0;
        self.current_scan_path.clear();

        scan_directory(&path, tx);
    }

    fn process_scan_messages(&mut self) {
        if let Some(ref rx) = self.scan_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ScanMessage::Progress {
                        files_scanned,
                        bytes_scanned,
                        current_path,
                    } => {
                        self.files_scanned = files_scanned;
                        self.bytes_scanned = bytes_scanned;
                        self.current_scan_path = current_path;
                    }
                    ScanMessage::Snapshot(mut snapshot) => {
                        self.apply_trashed_paths(&mut snapshot);
                        self.bytes_scanned = snapshot.get(snapshot.root).size;
                        self.files_scanned = snapshot.len() as u64;
                        let was_at_root = self.current_root == 0;
                        self.tree = Some(snapshot);
                        if was_at_root || self.state == AppState::Scanning {
                            self.current_root = 0;
                        }
                        self.layout_cache = None;
                        self.category_cache = None;
                        self.state = AppState::Viewing;
                    }
                    ScanMessage::Complete(mut tree) => {
                        self.apply_trashed_paths(&mut tree);
                        self.trashed_paths.clear();

                        if tree.len() <= 1 {
                            if self.tree.is_none() {
                                self.state = AppState::Error(
                                    "No files found (directory may be empty or access denied)"
                                        .to_string(),
                                );
                            } else {
                                self.show_toast("Scan returned no results (access denied?)");
                                self.state = AppState::Viewing;
                            }
                            self.scan_receiver = None;
                            return;
                        }

                        self.files_scanned = tree.len() as u64;
                        self.bytes_scanned = tree.get(tree.root).size;

                        let tree_for_cache = tree.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = cache::save(&tree_for_cache) {
                                eprintln!("Cache save failed: {}", e);
                            }
                        });

                        self.tree = Some(tree);
                        self.current_root = 0;
                        self.navigation_history.clear();
                        self.layout_cache = None;
                        self.category_cache = None;
                        self.state = AppState::Viewing;
                        self.scan_receiver = None;
                        return;
                    }
                    ScanMessage::Error(err) => {
                        if self.tree.is_none() {
                            self.state = AppState::Error(err);
                        }
                        self.scan_receiver = None;
                        return;
                    }
                }
            }
        }
    }

    fn apply_trashed_paths(&self, tree: &mut FsTree) {
        if self.trashed_paths.is_empty() {
            return;
        }
        let indices_to_remove: Vec<usize> = tree
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| self.trashed_paths.contains(&node.path))
            .map(|(i, _)| i)
            .collect();
        for idx in indices_to_remove {
            tree.remove_node(idx);
        }
        if !self.trashed_paths.is_empty() {
            tree.sort_children_by_size();
        }
    }

    fn is_scanning(&self) -> bool {
        self.scan_receiver.is_some()
    }
}

// --- Navigation ---

impl DiskApp {
    fn navigate_to(&mut self, index: usize) {
        self.navigation_history.push(self.current_root);
        self.current_root = index;
        self.invalidate_caches();
    }

    fn navigate_back(&mut self) {
        if let Some(prev) = self.navigation_history.pop() {
            self.current_root = prev;
            self.invalidate_caches();
        }
    }

    fn navigate_to_direct(&mut self, index: usize) {
        self.navigation_history.push(self.current_root);
        self.current_root = index;
        self.invalidate_caches();
    }

    fn invalidate_caches(&mut self) {
        self.layout_cache = None;
        self.category_cache = None;
    }
}

// --- UI helpers ---

impl DiskApp {
    fn has_blocking_overlay(&self) -> bool {
        self.context_menu_node.is_some() || self.trash_confirm.is_some()
    }

    fn handle_treemap_response(&mut self, response: treemap_view::TreemapResponse) {
        if self.has_blocking_overlay() {
            return;
        }

        if let Some(dir_idx) = response.clicked_dir {
            self.navigate_to(dir_idx);
        }
        if let Some(node_idx) = response.right_clicked {
            self.context_menu_node = Some(node_idx);
        }
    }

    fn show_toast(&mut self, msg: &str) {
        self.toast_message = Some((msg.to_string(), std::time::Instant::now()));
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Select Directory").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.start_scan(path);
                    }
                }

                if self.state == AppState::Viewing {
                    ui.separator();
                    if ui.button("Back").clicked() {
                        self.navigate_back();
                    }
                    if ui.button("Home").clicked() {
                        self.navigation_history.push(self.current_root);
                        self.current_root = 0;
                        self.invalidate_caches();
                    }
                    if ui.button("Rescan").clicked() {
                        if let Some(root) = self.scan_root.clone() {
                            self.start_scan(root);
                        }
                    }

                    ui.separator();

                    if let Some(ref tree) = self.tree {
                        if let Some(target) =
                            breadcrumbs::draw_breadcrumbs(ui, tree, self.current_root)
                        {
                            self.navigate_to_direct(target);
                        }
                    }
                }

                // Right-aligned: search box and scanning indicator
                if self.state == AppState::Viewing {
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if self.is_scanning() {
                                ui.spinner();
                                ui.label("Scanning...");
                            }

                            let prev_query = self.search_query.clone();
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.search_query)
                                    .hint_text("Search files...")
                                    .desired_width(180.0),
                            );
                            if response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                self.search_query.clear();
                                self.search_results.clear();
                                response.surrender_focus();
                            }
                            if self.search_query != prev_query {
                                if let Some(ref tree) = self.tree {
                                    self.search_results =
                                        tree.search(&self.search_query, SEARCH_MAX_RESULTS);
                                }
                            }
                        },
                    );
                }
            });
        });
    }

    fn draw_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            status_bar::draw_status_bar(
                ui,
                self.files_scanned,
                self.bytes_scanned,
                &self.current_scan_path,
                self.is_scanning(),
            );
        });
    }

    fn draw_sidebar(&mut self, ctx: &egui::Context) {
        if self.state != AppState::Viewing {
            return;
        }
        egui::SidePanel::left("sidebar")
            .default_width(SIDEBAR_DEFAULT_WIDTH)
            .show(ctx, |ui| {
                if let Some(ref tree) = self.tree {
                    let cat_sizes = match &self.category_cache {
                        Some((root, data)) if *root == self.current_root => data.clone(),
                        _ => {
                            let data =
                                sidebar::compute_category_sizes(tree, self.current_root);
                            self.category_cache =
                                Some((self.current_root, data.clone()));
                            data
                        }
                    };
                    if let Some(target) = sidebar::draw_sidebar(
                        ui,
                        tree,
                        self.current_root,
                        &self.search_results,
                        &cat_sizes,
                    ) {
                        self.navigate_to(target);
                    }
                }
            });
    }

    fn draw_central_panel(&mut self, ctx: &egui::Context) {
        let mut reset_to_welcome = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.state {
                AppState::Welcome => {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 3.0);
                            ui.heading("diske");
                            ui.add_space(8.0);
                            ui.label("Visual disk usage analyzer");
                            ui.add_space(16.0);
                            ui.label("Click \"Select Directory\" to start scanning");
                        });
                    });
                }
                AppState::Scanning => {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 3.0);
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.heading("Scanning...");
                            ui.label(format!(
                                "{} files found ({:.1} MB)",
                                self.files_scanned,
                                self.bytes_scanned as f64 / 1_048_576.0,
                            ));
                        });
                    });
                }
                AppState::Viewing => {
                    let tree = self.tree.as_ref().unwrap();
                    let highlighted: HashSet<usize> = if self.search_query.is_empty() {
                        HashSet::new()
                    } else {
                        let children: HashSet<usize> =
                            tree.children_of(self.current_root).iter().copied().collect();
                        self.search_results
                            .iter()
                            .filter_map(|&idx| {
                                if children.contains(&idx) {
                                    return Some(idx);
                                }
                                for &anc in &tree.ancestors(idx) {
                                    if children.contains(&anc) {
                                        return Some(anc);
                                    }
                                }
                                None
                            })
                            .collect()
                    };
                    let response = treemap_view::draw_treemap(
                        ui,
                        tree,
                        self.current_root,
                        &mut self.layout_cache,
                        &highlighted,
                    );
                    self.handle_treemap_response(response);
                }
                AppState::Error(msg) => {
                    let msg = msg.clone();
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 3.0);
                            ui.colored_label(egui::Color32::RED, format!("Error: {}", msg));
                            ui.add_space(16.0);
                            if ui.button("Try Again").clicked() {
                                reset_to_welcome = true;
                            }
                        });
                    });
                }
            }
        });
        if reset_to_welcome {
            self.state = AppState::Welcome;
        }
    }

    fn draw_context_menu(&mut self, ctx: &egui::Context) {
        let mut action = ContextAction::None;

        if let Some(node_idx) = self.context_menu_node {
            if let Some(ref tree) = self.tree {
                let node = tree.get(node_idx);
                let path = node.path.clone();
                let name = node.name.clone();

                let modal_response = egui::Modal::new(egui::Id::new("context_menu_modal"))
                    .show(ctx, |ui| {
                        ui.heading(format!("Actions: {}", name));
                        ui.separator();

                        if ui.button("Reveal in Finder").clicked() {
                            action = ContextAction::Reveal(path.clone());
                        }
                        if ui.button("Copy Path").clicked() {
                            action =
                                ContextAction::CopyPath(path.to_string_lossy().to_string());
                        }
                        ui.separator();
                        if ui
                            .button(
                                egui::RichText::new("Move to Trash")
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            action =
                                ContextAction::ConfirmTrash(path.clone(), name.clone(), node_idx);
                        }
                    });

                if modal_response.should_close() {
                    self.context_menu_node = None;
                }
            }
        }

        match action {
            ContextAction::Reveal(path) => {
                if let Err(e) = Command::new("open").arg("-R").arg(&path).spawn() {
                    self.show_toast(&format!("Failed to reveal: {}", e));
                }
                self.context_menu_node = None;
            }
            ContextAction::CopyPath(path_str) => {
                ctx.copy_text(path_str);
                self.show_toast("Path copied!");
                self.context_menu_node = None;
            }
            ContextAction::ConfirmTrash(path, name, node_idx) => {
                self.trash_confirm = Some((path, name, node_idx));
                self.context_menu_node = None;
            }
            ContextAction::None => {}
        }
    }

    fn draw_trash_confirm(&mut self, ctx: &egui::Context) {
        let mut trash_action: Option<(PathBuf, String, usize)> = None;
        let mut trash_cancel = false;

        if let Some((ref path, ref name, node_idx)) = self.trash_confirm {
            let size_text = self
                .tree
                .as_ref()
                .map(|t| {
                    let s = t.get(node_idx).size;
                    if s >= 1_073_741_824 {
                        format!("{:.1} GB", s as f64 / 1_073_741_824.0)
                    } else if s >= 1_048_576 {
                        format!("{:.1} MB", s as f64 / 1_048_576.0)
                    } else {
                        format!("{:.1} KB", s as f64 / 1024.0)
                    }
                })
                .unwrap_or_default();

            let modal_response = egui::Modal::new(egui::Id::new("trash_confirm_modal"))
                .show(ctx, |ui| {
                    ui.heading("Confirm Move to Trash");
                    ui.separator();

                    ui.label(format!("Move \"{}\" ({}) to Trash?", name, size_text));
                    ui.label(
                        egui::RichText::new(path.to_string_lossy().to_string())
                            .small()
                            .weak(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            trash_cancel = true;
                        }
                        if ui
                            .button(
                                egui::RichText::new("Move to Trash")
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            trash_action = Some((path.clone(), name.clone(), node_idx));
                        }
                    });
                });

            if modal_response.should_close() {
                trash_cancel = true;
            }
        }

        if trash_cancel {
            self.trash_confirm = None;
        }
        if let Some((path, name, node_idx)) = trash_action {
            self.execute_trash(path, name, node_idx);
        }
    }

    fn execute_trash(&mut self, path: PathBuf, name: String, node_idx: usize) {
        match trash::delete(&path) {
            Ok(()) => {
                self.trashed_paths.insert(path);
                if let Some(ref mut tree) = self.tree {
                    tree.remove_node(node_idx);
                    tree.sort_children_by_size();

                    let tree_for_cache = tree.clone();
                    std::thread::spawn(move || {
                        let _ = cache::save(&tree_for_cache);
                    });
                }
                self.invalidate_caches();
                self.show_toast(&format!("Moved to Trash: {}", name));
            }
            Err(e) => {
                self.show_toast(&format!("Failed to trash: {}", e));
            }
        }
        self.trash_confirm = None;
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        if let Some((ref msg, instant)) = self.toast_message {
            let elapsed = instant.elapsed().as_secs_f32();
            if elapsed < TOAST_DURATION_SECS {
                let alpha = if elapsed > TOAST_FADE_START_SECS {
                    ((TOAST_DURATION_SECS - elapsed) * 255.0) as u8
                } else {
                    255
                };
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -40.0])
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgba_unmultiplied(40, 40, 40, alpha))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(msg).color(
                                        egui::Color32::from_rgba_unmultiplied(
                                            255, 255, 255, alpha,
                                        ),
                                    ),
                                );
                            });
                    });
                ctx.request_repaint();
            } else {
                self.toast_message = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> DiskApp {
        DiskApp {
            state: AppState::Viewing,
            tree: None,
            current_root: 1,
            navigation_history: Vec::new(),
            scan_receiver: None,
            scan_root: None,
            files_scanned: 0,
            bytes_scanned: 0,
            current_scan_path: String::new(),
            layout_cache: None,
            context_menu_node: None,
            trash_confirm: None,
            trashed_paths: HashSet::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            category_cache: None,
            toast_message: None,
        }
    }

    #[test]
    fn treemap_click_navigates_when_no_overlay_is_open() {
        let mut app = test_app();

        app.handle_treemap_response(treemap_view::TreemapResponse {
            clicked_dir: Some(7),
            right_clicked: None,
        });

        assert_eq!(app.current_root, 7);
        assert_eq!(app.navigation_history, vec![1]);
    }

    #[test]
    fn treemap_click_is_ignored_while_context_menu_is_open() {
        let mut app = test_app();
        app.context_menu_node = Some(99);

        app.handle_treemap_response(treemap_view::TreemapResponse {
            clicked_dir: Some(7),
            right_clicked: None,
        });

        assert_eq!(app.current_root, 1);
        assert!(app.navigation_history.is_empty());
        assert_eq!(app.context_menu_node, Some(99));
    }

    #[test]
    fn treemap_right_click_is_ignored_while_trash_dialog_is_open() {
        let mut app = test_app();
        app.trash_confirm = Some((PathBuf::from("/tmp/file"), "file".to_string(), 5));

        app.handle_treemap_response(treemap_view::TreemapResponse {
            clicked_dir: None,
            right_clicked: Some(7),
        });

        assert_eq!(app.context_menu_node, None);
    }
}

// --- Main update loop ---

impl eframe::App for DiskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.is_scanning() {
            self.process_scan_messages();
            ctx.request_repaint();
        }

        // Keyboard shortcuts (only when no text field is focused)
        if self.state == AppState::Viewing && !ctx.wants_keyboard_input() {
            if ctx.input(|i| {
                i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Backspace)
            }) {
                self.navigate_back();
            }
        }

        self.draw_top_bar(ctx);
        self.draw_status_bar(ctx);
        self.draw_sidebar(ctx);
        self.draw_central_panel(ctx);
        self.draw_context_menu(ctx);
        self.draw_trash_confirm(ctx);
        self.draw_toast(ctx);
    }
}
