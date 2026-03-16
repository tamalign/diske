/// Draw the status bar showing scan progress.
pub fn draw_status_bar(
    ui: &mut egui::Ui,
    files_scanned: u64,
    bytes_scanned: u64,
    current_path: &str,
    is_scanning: bool,
) {
    ui.horizontal(|ui| {
        if is_scanning {
            ui.spinner();
            ui.label(format!(
                "Scanning... {} files ({}) — {}",
                format_count(files_scanned),
                format_size(bytes_scanned),
                truncate_path(current_path, 60),
            ));
        } else if files_scanned > 0 {
            ui.label(format!(
                "Scan complete: {} files ({})",
                format_count(files_scanned),
                format_size(bytes_scanned),
            ));
        }
    });
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
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

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
