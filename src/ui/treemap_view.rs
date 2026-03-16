use egui::{Color32, FontId, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::scan::fs_tree::FsTree;
use crate::treemap::layout::{squarify, LayoutRect};
use crate::ui::colors::{color_for_node, darken, lighten};

const TREEMAP_PADDING: f32 = 1.5;
const FONT_SIZE_LARGE: f32 = 13.0;
const FONT_SIZE_SMALL: f32 = 10.0;
const MIN_RECT_LABEL_WIDTH: f32 = 40.0;
const MIN_RECT_LABEL_HEIGHT: f32 = 20.0;
const MIN_RECT_SIZE_LABEL_HEIGHT: f32 = 36.0;
const LARGE_FONT_MIN_WIDTH: f32 = 100.0;
const LARGE_FONT_MIN_HEIGHT: f32 = 30.0;

pub struct TreemapResponse {
    pub clicked_dir: Option<usize>,
    pub right_clicked: Option<usize>,
}

/// Render the treemap for the given node's children.
pub fn draw_treemap(
    ui: &mut egui::Ui,
    tree: &FsTree,
    current_root: usize,
    layout_cache: &mut Option<(usize, Vec2, Vec<LayoutRect>)>,
) -> TreemapResponse {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, Sense::hover());
    let rect = response.rect;

    // Get children items sorted by size
    let children = tree.children_of(current_root);
    let items: Vec<(usize, u64)> = children
        .iter()
        .filter(|&&idx| tree.get(idx).size > 0)
        .map(|&idx| (idx, tree.get(idx).size))
        .collect();

    // Compute or reuse layout
    let layout = if let Some((cached_root, cached_size, cached_layout)) = layout_cache {
        if *cached_root == current_root
            && (*cached_size - available).length() < 1.0
        {
            cached_layout.clone()
        } else {
            let new_layout = squarify(
                &items,
                (rect.min.x, rect.min.y, rect.width(), rect.height()),
                TREEMAP_PADDING,
            );
            *layout_cache = Some((current_root, available, new_layout.clone()));
            new_layout
        }
    } else {
        let new_layout = squarify(
            &items,
            (rect.min.x, rect.min.y, rect.width(), rect.height()),
            1.5,
        );
        *layout_cache = Some((current_root, available, new_layout.clone()));
        new_layout
    };

    let mouse_pos = ui.input(|i| i.pointer.hover_pos());

    let mut result = TreemapResponse {
        clicked_dir: None,
        right_clicked: None,
    };

    let mut hovered_index: Option<usize> = None;

    // Draw rectangles
    for lr in &layout {
        let node = tree.get(lr.node_index);
        let item_rect = Rect::from_min_size(
            Pos2::new(lr.x, lr.y),
            Vec2::new(lr.w, lr.h),
        );

        if item_rect.width() < 1.0 || item_rect.height() < 1.0 {
            continue;
        }

        let base_color = color_for_node(tree, lr.node_index);

        let is_hovered = mouse_pos
            .map(|p| item_rect.contains(p))
            .unwrap_or(false);

        let fill_color = if is_hovered {
            hovered_index = Some(lr.node_index);
            lighten(base_color, 0.2)
        } else {
            base_color
        };

        // Fill
        painter.rect_filled(item_rect, 2.0, fill_color);

        // Border
        if is_hovered {
            painter.rect_stroke(item_rect, 2.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
        } else {
            painter.rect_stroke(
                item_rect,
                2.0,
                Stroke::new(0.5, darken(base_color, 0.3)),
                StrokeKind::Inside,
            );
        }

        // Label (only if rectangle is large enough)
        if item_rect.width() > MIN_RECT_LABEL_WIDTH && item_rect.height() > MIN_RECT_LABEL_HEIGHT {
            let text = &node.name;
            let font = FontId::proportional(if item_rect.width() > LARGE_FONT_MIN_WIDTH && item_rect.height() > LARGE_FONT_MIN_HEIGHT {
                FONT_SIZE_LARGE
            } else {
                FONT_SIZE_SMALL
            });

            // Clip text to fit (char-aware to handle multibyte)
            let max_chars = (item_rect.width() / 7.0) as usize;
            let char_count = text.chars().count();
            let display_text = if char_count > max_chars && max_chars > 3 {
                let truncated: String = text.chars().take(max_chars - 3).collect();
                format!("{}...", truncated)
            } else {
                text.clone()
            };

            let text_pos = Pos2::new(item_rect.min.x + 4.0, item_rect.min.y + 3.0);
            // Text shadow for readability
            let shadow_offset = Pos2::new(text_pos.x + 1.0, text_pos.y + 1.0);
            painter.text(
                shadow_offset,
                egui::Align2::LEFT_TOP,
                &display_text,
                font.clone(),
                Color32::from_rgba_premultiplied(0, 0, 0, 160),
            );
            painter.text(
                text_pos,
                egui::Align2::LEFT_TOP,
                &display_text,
                font.clone(),
                Color32::WHITE,
            );

            // Show size below name if enough space
            if item_rect.height() > MIN_RECT_SIZE_LABEL_HEIGHT {
                let size_text = format_size(node.size);
                let size_pos = Pos2::new(item_rect.min.x + 4.0, item_rect.min.y + 18.0);
                painter.text(
                    Pos2::new(size_pos.x + 1.0, size_pos.y + 1.0),
                    egui::Align2::LEFT_TOP,
                    &size_text,
                    FontId::proportional(10.0),
                    Color32::from_rgba_premultiplied(0, 0, 0, 120),
                );
                painter.text(
                    size_pos,
                    egui::Align2::LEFT_TOP,
                    &size_text,
                    FontId::proportional(10.0),
                    Color32::from_rgba_premultiplied(255, 255, 255, 200),
                );
            }
        }
    }

    // Handle clicks
    if let Some(hover_idx) = hovered_index {
        let node = tree.get(hover_idx);

        // Tooltip
        let layer_id = LayerId::new(Order::Tooltip, ui.id().with("treemap_layer"));
        egui::show_tooltip_at_pointer(ui.ctx(), layer_id, ui.id().with("treemap_tooltip"), |ui: &mut egui::Ui| {
            ui.label(format!("{}", node.path.display()));
            ui.label(format!("Size: {}", format_size(node.size)));
            if node.is_dir {
                ui.label(format!(
                    "Items: {}",
                    tree.children_of(hover_idx).len()
                ));
                ui.small("Click to open");
            }
        });

        // Left click on directory -> navigate into it
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
            if node.is_dir {
                result.clicked_dir = Some(hover_idx);
            }
        }

        // Right click -> context menu
        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary)) {
            result.right_clicked = Some(hover_idx);
        }
    }

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
