pub mod models;
pub mod scanner;

pub use models::Track;
pub use scanner::{load_cover, scan_folder};
