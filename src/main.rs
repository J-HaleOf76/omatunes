mod app;
mod audio;
mod library;
mod ui;

fn main() -> iced::Result {
    ui::theme::load_system_theme();
    app::run()
}
