mod app;
mod scan;
mod treemap;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("diske — Disk Usage Analyzer"),
        ..Default::default()
    };

    eframe::run_native(
        "diske",
        options,
        Box::new(|cc| Ok(Box::new(app::DiskApp::new(cc)))),
    )
}
