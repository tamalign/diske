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
use crate::scan::walker::{ScanMessage, scan_directory};
use crate::treemap::layout::LayoutRect;
use crate::ui::{breadcrumbs, sidebar, status_bar, treemap_view};

#[derive(PartialEq)]
enum AppState {
    Welcome,
    Scanning,
    Viewing,
    Error(String),
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

    // Paths trashed during current scan (to filter from incoming scan results)
    trashed_paths: HashSet<PathBuf>,

    // Toast message
    toast_message: Option<(String, std::time::Instant)>,
}

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
            trashed_paths: HashSet::new(),
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
            // Always start a fresh scan (will replace cache when done)
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
        self.navigation_history.clear();
        self.current_root = 0;
        self.trashed_paths.clear();

        scan_directory(&path, tx);
    }

    /// Start a scan but keep current tree visible (for background rescan with cache)
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
                        // Remove any items trashed during this scan
                        self.apply_trashed_paths(&mut snapshot);
                        // Update treemap with intermediate results during scan
                        self.bytes_scanned = snapshot.get(snapshot.root).size;
                        self.files_scanned = snapshot.len() as u64;
                        // Reset navigation to root when updating snapshot
                        let was_at_root = self.current_root == 0;
                        self.tree = Some(snapshot);
                        if was_at_root || self.state == AppState::Scanning {
                            self.current_root = 0;
                        }
                        self.layout_cache = None;
                        self.state = AppState::Viewing;
                    }
                    ScanMessage::Complete(mut tree) => {
                        // Remove any items trashed during this scan
                        self.apply_trashed_paths(&mut tree);
                        // Clear trashed paths — scan is done, next scan won't include them
                        self.trashed_paths.clear();

                        // If scan returned only the root node (empty/no access), show error
                        if tree.len() <= 1 {
                            if self.tree.is_none() {
                                self.state = AppState::Error(
                                    "No files found (directory may be empty or access denied)".to_string(),
                                );
                            } else {
                                // Keep existing tree, just show toast
                                self.toast_message = Some((
                                    "Scan returned no results (access denied?)".to_string(),
                                    std::time::Instant::now(),
                                ));
                                self.state = AppState::Viewing;
                            }
                            self.scan_receiver = None;
                            return;
                        }

                        self.files_scanned = tree.len() as u64;
                        self.bytes_scanned = tree.get(tree.root).size;

                        // Save to cache in background
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
                        self.state = AppState::Viewing;
                        self.scan_receiver = None;
                        return;
                    }
                    ScanMessage::Error(err) => {
                        // Only show error if we don't have a tree to display
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

    fn navigate_to(&mut self, index: usize) {
        self.navigation_history.push(self.current_root);
        self.current_root = index;
        self.layout_cache = None;
    }

    fn navigate_back(&mut self) {
        if let Some(prev) = self.navigation_history.pop() {
            self.current_root = prev;
            self.layout_cache = None;
        }
    }

    fn navigate_to_direct(&mut self, index: usize) {
        self.navigation_history.push(self.current_root);
        self.current_root = index;
        self.layout_cache = None;
    }

    /// Remove trashed paths from an incoming scan tree so deleted items don't reappear.
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

impl eframe::App for DiskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process scan messages
        if self.is_scanning() {
            self.process_scan_messages();
            ctx.request_repaint();
        }

        // Keyboard shortcuts
        if self.state == AppState::Viewing {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Backspace)) {
                self.navigate_back();
            }
        }

        // Top bar
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
                        self.layout_cache = None;
                    }
                    if ui.button("Rescan").clicked() {
                        if let Some(root) = self.scan_root.clone() {
                            self.start_scan(root);
                        }
                    }

                    ui.separator();

                    if let Some(ref tree) = self.tree {
                        if let Some(target) = breadcrumbs::draw_breadcrumbs(ui, tree, self.current_root) {
                            self.navigate_to_direct(target);
                        }
                    }
                }

                // Show scanning indicator in top bar
                if self.is_scanning() && self.state == AppState::Viewing {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spinner();
                        ui.label("Scanning...");
                    });
                }
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            status_bar::draw_status_bar(
                ui,
                self.files_scanned,
                self.bytes_scanned,
                &self.current_scan_path,
                self.is_scanning(),
            );
        });

        // Sidebar
        if self.state == AppState::Viewing {
            egui::SidePanel::left("sidebar")
                .default_width(SIDEBAR_DEFAULT_WIDTH)
                .show(ctx, |ui| {
                    if let Some(ref tree) = self.tree {
                        if let Some(target) = sidebar::draw_sidebar(ui, tree, self.current_root) {
                            self.navigate_to(target);
                        }
                    }
                });
        }

        // Central panel
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
                    let response = treemap_view::draw_treemap(
                        ui,
                        tree,
                        self.current_root,
                        &mut self.layout_cache,
                    );

                    if let Some(dir_idx) = response.clicked_dir {
                        self.navigate_to(dir_idx);
                    }

                    if let Some(node_idx) = response.right_clicked {
                        self.context_menu_node = Some(node_idx);
                    }
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

        // Context menu — collect action to execute after immutable borrow ends
        enum ContextAction {
            Reveal(PathBuf),
            CopyPath(String),
            Trash(PathBuf, String, usize),
            None,
        }
        let mut action = ContextAction::None;

        if let Some(node_idx) = self.context_menu_node {
            if let Some(ref tree) = self.tree {
                let node = tree.get(node_idx);
                let path = node.path.clone();
                let name = node.name.clone();

                let mut open = true;
                egui::Window::new(format!("Actions: {}", name))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        if ui.button("Reveal in Finder").clicked() {
                            action = ContextAction::Reveal(path.clone());
                        }
                        if ui.button("Copy Path").clicked() {
                            action = ContextAction::CopyPath(path.to_string_lossy().to_string());
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new("Move to Trash").color(egui::Color32::RED))
                            .clicked()
                        {
                            action = ContextAction::Trash(path.clone(), name.clone(), node_idx);
                        }
                    });

                if !open {
                    self.context_menu_node = None;
                }
            }
        }

        // Execute context action outside the borrow
        match action {
            ContextAction::Reveal(path) => {
                match Command::new("open").arg("-R").arg(&path).spawn() {
                    Ok(_) => {}
                    Err(e) => {
                        self.toast_message = Some((
                            format!("Failed to reveal: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                }
                self.context_menu_node = None;
            }
            ContextAction::CopyPath(path_str) => {
                ctx.copy_text(path_str);
                self.toast_message = Some((
                    "Path copied!".to_string(),
                    std::time::Instant::now(),
                ));
                self.context_menu_node = None;
            }
            ContextAction::Trash(path, name, node_idx) => {
                match trash::delete(&path) {
                    Ok(()) => {
                        // Track trashed path so incoming scan results won't restore it
                        self.trashed_paths.insert(path.clone());
                        if let Some(ref mut tree) = self.tree {
                            tree.remove_node(node_idx);
                            tree.sort_children_by_size();
                            self.layout_cache = None;

                            // Update disk cache with the modified tree
                            let tree_for_cache = tree.clone();
                            std::thread::spawn(move || {
                                let _ = cache::save(&tree_for_cache);
                            });
                        }
                        self.toast_message = Some((
                            format!("Moved to Trash: {}", name),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.toast_message = Some((
                            format!("Failed to trash: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                }
                self.context_menu_node = None;
            }
            ContextAction::None => {}
        }

        // Toast notification
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
                                    egui::RichText::new(msg)
                                        .color(egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)),
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
