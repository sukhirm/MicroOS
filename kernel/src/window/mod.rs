//! Micro-OS Window Manager Core (Phase 4C).
//!
//! Provides modular window abstractions, interactive window chrome rendering,
//! Z-order stacking, rectangular occlusion clipping, hit testing, and state management.

pub mod demo;
pub mod manager;
pub mod renderer;
pub mod state;
pub mod tests;
pub mod window;

#[allow(unused_imports)]
pub use demo::create_demo_desktop;
#[allow(unused_imports)]
pub use manager::WindowManager;
#[allow(unused_imports)]
pub use renderer::render_window;
#[allow(unused_imports)]
pub use state::{WindowControl, WindowId, WindowState, WindowTheme};
pub use tests::run_self_tests;
#[allow(unused_imports)]
pub use window::Window;
