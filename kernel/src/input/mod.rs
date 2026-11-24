//! Micro-OS Input Subsystem (Phase 4D).
//!
//! Provides PS/2 keyboard and mouse drivers, an IRQ-safe bounded event queue,
//! scancode and packet decoders, desktop event dispatching, window dragging,
//! and keyboard shortcuts.

pub mod controller;
pub mod dispatcher;
pub mod event;
pub mod keyboard;
pub mod mouse;
pub mod queue;
pub mod tests;

#[allow(unused_imports)]
pub use dispatcher::EventDispatcher;
#[allow(unused_imports)]
pub use event::{
    ButtonAction, InputEvent, KeyAction, KeyCode, KeyEvent, KeyModifiers, MouseButton,
    MouseButtons, MouseEvent,
};
#[allow(unused_imports)]
pub use controller::poll_hardware;
#[allow(unused_imports)]
pub use queue::{clear_events, event_overruns, pending_events, pop_event, push_event};
pub use tests::run_self_tests;

/// Initializes the hardware input subsystem (8042 controller, keyboard, mouse, and PIC unmasking).
pub fn init(screen_width: u32, screen_height: u32) {
    log::info!("Initializing Phase 4D Input Subsystem...");

    // 1. Configure mouse screen dimensions
    mouse::set_screen_dimensions(screen_width, screen_height);

    // 2. Initialize 8042 PS/2 controller hardware
    if let Err(err) = controller::init() {
        log::warn!("[Input] 8042 Controller initialization warning: {}", err);
    }

    // 3. Unmask IRQ1 (Keyboard) and IRQ12 (Mouse via IRQ2 cascade) on the 8259 PIC
    unsafe {
        crate::arch::x86_64::pic::enable_input_irqs();
    }

    log::info!("[Input] Keyboard & Mouse input subsystem active.");
}
