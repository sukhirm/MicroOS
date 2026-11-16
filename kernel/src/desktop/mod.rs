//! Desktop Environment Shell for Micro-OS.
//!
//! Provides the visual identity, wallpaper, top status bar, bottom application dock,
//! and software mouse cursor for the Cosmic Slate / Obsidian Azure graphical desktop.

pub mod cursor;
pub mod dock;
pub mod shell;
pub mod status_bar;
pub mod tests;
pub mod wallpaper;

#[allow(unused_imports)]
pub use cursor::Cursor;
#[allow(unused_imports)]
pub use dock::{get_dock_bounds, render_dock, DOCK_ITEMS};
#[allow(unused_imports)]
pub use shell::{render_desktop, render_desktop_with_wm, DesktopShell};
#[allow(unused_imports)]
pub use status_bar::{render_status_bar, StatusMetrics, STATUS_BAR_HEIGHT};
pub use tests::run_self_tests;
#[allow(unused_imports)]
pub use wallpaper::render_wallpaper;
