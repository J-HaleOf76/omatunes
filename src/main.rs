mod app;
mod audio;
mod library;
mod locale;
mod ui;

fn main() -> iced::Result {
    locale::load();
    ui::theme::load_system_theme();
    app::run()
}
