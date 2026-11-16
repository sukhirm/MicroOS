//! Graphics Subsystem and 2D Rendering Engine for Micro-OS.
//!
//! Provides surface abstractions, hardware framebuffer management, double-buffering,
//! clipping-aware 2D geometry primitives, multi-scale typography, and embedded bitmap icons
//! adhering to the Cosmic Slate / Obsidian Azure visual identity.

pub mod color;
pub mod demo;
pub mod double_buffer;
pub mod font;
pub mod geometry;
pub mod icon;
pub mod surface;
pub mod tests;

#[allow(unused_imports)]
pub use color::Color;
#[allow(unused_imports)]
pub use demo::render_demo;
#[allow(unused_imports)]
pub use double_buffer::{create_backbuffer_surface, present, present_rect};
#[allow(unused_imports)]
pub use font::{draw_char, draw_string, measure_string};
#[allow(unused_imports)]
pub use geometry::{Point, Rect};
#[allow(unused_imports)]
pub use icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_CHECK, ICON_CLOSE, ICON_FOLDER, ICON_GEAR,
    ICON_NETWORK, ICON_STORE, ICON_TERMINAL,
};
#[allow(unused_imports)]
pub use surface::Surface;
pub use tests::run_self_tests;
