use crossbeam_channel::{Receiver, unbounded};
use egui::Vec2;
use std::path::PathBuf;

use crate::scan::fs_tree::FsTree;
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
    files_scanned: u64,
    bytes_scanned: u64,
    current_scan_path: String,

    // Layout cache
    layout_cache: Option<(usize, Vec2, Vec<LayoutRect>)>,

    // Context menu
    context_menu_node: Option<usize>,
}

impl DiskApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Force dark theme
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self {
            state: AppState::Welcome,
            tree: None,
            current_root: 0,
            navigation_history: Vec::new(),
            scan_receiver: None,
            files_scanned: 0,
            bytes_scanned: 0,
            current_scan_path: String::new(),
            layout_cache: None,
            context_menu_node: None,
        };

        // Auto-scan home directory on launch
        if let Some(home) = dirs_next::home_dir() {
            app.start_scan(home);
        }

        app
    }

    fn start_scan(&mut self, path: PathBuf) {
        let (tx, rx) = unbounded();
        self.scan_receiver = Some(rx);
        self.state = AppState::Scanning;
        self.files_scanned = 0;
        self.bytes_scanned = 0;
        self.current_scan_path.clear();
        self.tree = None;
        self.layout_cache = None;
        self.navigation_history.clear();
        self.current_root = 0;

        scan_directory(&path, tx);
    }

    fn process_scan_messages(&mut self) {
        if let Some(ref rx) = self.scan_receiver {
            // Drain all available messages
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
                    ScanMessage::Complete(tree) => {
                        self.files_scanned = tree.len() as u64;
                        self.bytes_scanned = tree.get(tree.root).size;
                        self.tree = Some(tree);
                        self.current_root = 0;
                        self.state = AppState::Viewing;
                        self.scan_receiver = None;
                        return;
                    }
                    ScanMessage::Error(err) => {
                        self.state = AppState::Error(err);
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
        // Navigate directly (for breadcrumbs), clear forward history
        self.navigation_history.push(self.current_root);
        self.current_root = index;
        self.layout_cache = None;
    }
}

impl eframe::App for DiskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process scan messages
        if self.state == AppState::Scanning {
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

                    ui.separator();

                    // Breadcrumbs
                    if let Some(ref tree) = self.tree {
                        if let Some(target) = breadcrumbs::draw_breadcrumbs(ui, tree, self.current_root) {
                            self.navigate_to_direct(target);
                        }
                    }
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
                self.state == AppState::Scanning,
            );
        });

        // Sidebar
        if self.state == AppState::Viewing {
            egui::SidePanel::left("sidebar")
                .default_width(220.0)
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

        // Context menu (shown as a window)
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
                            let _ = open::that_detached(path.parent().unwrap_or(&path));
                            self.context_menu_node = None;
                        }
                        if ui.button("Copy Path").clicked() {
                            ui.ctx().copy_text(path.to_string_lossy().to_string());
                            self.context_menu_node = None;
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new("Move to Trash").color(egui::Color32::RED))
                            .clicked()
                        {
                            match trash::delete(&path) {
                                Ok(()) => {
                                    // Remove from tree and update
                                    if let Some(ref mut tree) = self.tree {
                                        tree.remove_node(node_idx);
                                        self.layout_cache = None;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to trash: {}", e);
                                }
                            }
                            self.context_menu_node = None;
                        }
                    });

                if !open {
                    self.context_menu_node = None;
                }
            }
        }
    }
}
