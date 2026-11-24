//! Micro-OS Settings Application ("Settings").
//!
//! Provides an interactive desktop control center with Cosmic Slate styling:
//! 1. Appearance: Theme options, Accent color selector (Cyan, Azure, Emerald, Amber), UI toggles (Desktop Grid, Transparency, Animations).
//! 2. Display: Hardware GOP framebuffer metrics, Pixel format, Stride, 16 MiB Backbuffer status, Display resolution notice.
//! 3. System Information: CPU Architecture (x86_64 Long Mode), Kernel version (v0.11.17), RAM (497 MiB usable), 16 MiB Heap, 16 MiB Backbuffer, PIT 100 Hz, Scheduler.
//! 4. Input: PS/2 Keyboard IRQ1, PS/2 Mouse IRQ12, 128-entry bounded ring buffer, Keyboard shortcuts reference.
//! 5. About MicroOS: Brand badge, Rust no_std, architecture, build profile, system vision.

use spin::Mutex;

use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_CHECK, ICON_FOLDER, ICON_GEAR, ICON_STORE, ICON_TERMINAL,
};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected Settings application state.
pub static SETTINGS_APP: Mutex<SettingsApp> = Mutex::new(SettingsApp::new());

/// The five distinct configuration sections of the Settings application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Display,
    SystemInfo,
    Input,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 5] = [
        SettingsSection::Appearance,
        SettingsSection::Display,
        SettingsSection::SystemInfo,
        SettingsSection::Input,
        SettingsSection::About,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            SettingsSection::Appearance => "Appearance",
            SettingsSection::Display => "Display",
            SettingsSection::SystemInfo => "System Info",
            SettingsSection::Input => "Input",
            SettingsSection::About => "About",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            SettingsSection::Appearance => &ICON_GEAR,
            SettingsSection::Display => &ICON_STORE,
            SettingsSection::SystemInfo => &ICON_ACTIVITY,
            SettingsSection::Input => &ICON_TERMINAL,
            SettingsSection::About => &ICON_FOLDER,
        }
    }
}

/// Available accent colour choices for the desktop user interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentColorChoice {
    Cyan,
    Azure,
    Emerald,
    Amber,
}

impl AccentColorChoice {
    pub const ALL: [AccentColorChoice; 4] = [
        AccentColorChoice::Cyan,
        AccentColorChoice::Azure,
        AccentColorChoice::Emerald,
        AccentColorChoice::Amber,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            AccentColorChoice::Cyan => "Cyan",
            AccentColorChoice::Azure => "Azure",
            AccentColorChoice::Emerald => "Emerald",
            AccentColorChoice::Amber => "Amber",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            AccentColorChoice::Cyan => Color::ACCENT_CYAN,
            AccentColorChoice::Azure => Color::ACCENT_AZURE,
            AccentColorChoice::Emerald => Color::SUCCESS_GREEN,
            AccentColorChoice::Amber => Color::WARNING_AMBER,
        }
    }
}

/// Global settings state machine and user preference values.
pub struct SettingsApp {
    /// Currently displayed settings section.
    pub current_section: SettingsSection,
    /// Currently selected accent colour choice.
    pub selected_accent: AccentColorChoice,
    /// Whether the desktop subtle grid pattern is enabled.
    pub desktop_grid_enabled: bool,
    /// Whether window transparency / acrylic effects are enabled.
    pub transparency_enabled: bool,
    /// Whether smooth window transitions/animations are enabled.
    pub animations_enabled: bool,
    /// Sidebar active selection index.
    pub selected_sidebar_index: usize,
}

impl SettingsApp {
    /// Creates a new default SettingsApp instance.
    pub const fn new() -> Self {
        Self {
            current_section: SettingsSection::Appearance,
            selected_accent: AccentColorChoice::Cyan,
            desktop_grid_enabled: true,
            transparency_enabled: true,
            animations_enabled: true,
            selected_sidebar_index: 0,
        }
    }

    /// Selects an explicit settings section.
    pub fn select_section(&mut self, section: SettingsSection) {
        self.current_section = section;
        if let Some(idx) = SettingsSection::ALL.iter().position(|&s| s == section) {
            self.selected_sidebar_index = idx;
        }
    }

    /// Selects an accent colour.
    pub fn select_accent(&mut self, accent: AccentColorChoice) {
        self.selected_accent = accent;
    }

    /// Toggles the desktop grid pattern setting.
    pub fn toggle_desktop_grid(&mut self) -> bool {
        self.desktop_grid_enabled = !self.desktop_grid_enabled;
        self.desktop_grid_enabled
    }

    /// Toggles the window transparency setting.
    pub fn toggle_transparency(&mut self) -> bool {
        self.transparency_enabled = !self.transparency_enabled;
        self.transparency_enabled
    }

    /// Toggles the smooth animations setting.
    pub fn toggle_animations(&mut self) -> bool {
        self.animations_enabled = !self.animations_enabled;
        self.animations_enabled
    }

    /// Handles mouse clicks within the Settings window client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        let sidebar_w = 130i32;

        // 1. Sidebar clicks (x: content_rect.x .. content_rect.x + 130)
        if pt.x >= content_rect.x && pt.x < content_rect.x + sidebar_w {
            let start_y = content_rect.y + 36;
            let item_h = 32i32;

            for (i, &sec) in SettingsSection::ALL.iter().enumerate() {
                let cur_y = start_y + (i as i32 * item_h);
                if pt.y >= cur_y && pt.y < cur_y + item_h {
                    self.select_section(sec);
                    return true;
                }
            }
            return false;
        }

        // 2. Client details area clicks (x >= content_rect.x + 130)
        if pt.x >= content_rect.x + sidebar_w {
            let main_x = content_rect.x + sidebar_w + 14;

            match self.current_section {
                SettingsSection::Appearance => {
                    // Check accent color swatches
                    let accent_y = content_rect.y + 70;
                    let swatch_w = 72i32;
                    let swatch_h = 28i32;
                    let gap = 8i32;

                    if pt.y >= accent_y && pt.y < accent_y + swatch_h {
                        for (i, &acc) in AccentColorChoice::ALL.iter().enumerate() {
                            let cur_x = main_x + (i as i32 * (swatch_w + gap));
                            if pt.x >= cur_x && pt.x < cur_x + swatch_w {
                                self.select_accent(acc);
                                return true;
                            }
                        }
                    }

                    // Check preferences toggle rows
                    let toggle_start_y = content_rect.y + 130;
                    let row_h = 28i32;
                    let row_gap = 6i32;

                    // Row 0: Desktop Grid
                    let r0_y = toggle_start_y;
                    if pt.y >= r0_y && pt.y < r0_y + row_h {
                        self.toggle_desktop_grid();
                        return true;
                    }

                    // Row 1: Window Transparency
                    let r1_y = toggle_start_y + row_h + row_gap;
                    if pt.y >= r1_y && pt.y < r1_y + row_h {
                        self.toggle_transparency();
                        return true;
                    }

                    // Row 2: Smooth Animations
                    let r2_y = toggle_start_y + (row_h + row_gap) * 2;
                    if pt.y >= r2_y && pt.y < r2_y + row_h {
                        self.toggle_animations();
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Handles keyboard events for the Settings window.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        match key.code {
            KeyCode::Down => {
                let next_idx = (self.selected_sidebar_index + 1) % SettingsSection::ALL.len();
                self.selected_sidebar_index = next_idx;
                self.current_section = SettingsSection::ALL[next_idx];
                true
            }
            KeyCode::Up => {
                let prev_idx = if self.selected_sidebar_index == 0 {
                    SettingsSection::ALL.len() - 1
                } else {
                    self.selected_sidebar_index - 1
                };
                self.selected_sidebar_index = prev_idx;
                self.current_section = SettingsSection::ALL[prev_idx];
                true
            }
            KeyCode::Right => {
                if self.current_section == SettingsSection::Appearance {
                    let next_accent = match self.selected_accent {
                        AccentColorChoice::Cyan => AccentColorChoice::Azure,
                        AccentColorChoice::Azure => AccentColorChoice::Emerald,
                        AccentColorChoice::Emerald => AccentColorChoice::Amber,
                        AccentColorChoice::Amber => AccentColorChoice::Cyan,
                    };
                    self.select_accent(next_accent);
                    return true;
                }
                false
            }
            KeyCode::Left => {
                if self.current_section == SettingsSection::Appearance {
                    let prev_accent = match self.selected_accent {
                        AccentColorChoice::Cyan => AccentColorChoice::Amber,
                        AccentColorChoice::Azure => AccentColorChoice::Cyan,
                        AccentColorChoice::Emerald => AccentColorChoice::Azure,
                        AccentColorChoice::Amber => AccentColorChoice::Emerald,
                    };
                    self.select_accent(prev_accent);
                    return true;
                }
                false
            }
            KeyCode::Space | KeyCode::Enter => {
                if self.current_section == SettingsSection::Appearance {
                    self.toggle_desktop_grid();
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Global client content renderer for the Settings window.
pub fn render_settings_content(_window: &Window, surface: &mut Surface, content_rect: Rect) {
    let app = SETTINGS_APP.lock();

    let saved_clip = surface.clip_rect();
    let client_clip = match saved_clip.intersect(&content_rect) {
        Some(c) => c,
        None => return,
    };
    surface.set_clip_rect(client_clip);

    // 1. Fill base client background (Obsidian Slate)
    surface.fill_rect(content_rect, Color::new(0x0E, 0x14, 0x24));

    // 2. Render Left Navigation Sidebar (Width: 130 px)
    let sidebar_w = 130u32;
    let sidebar_rect = Rect::new(content_rect.x, content_rect.y, sidebar_w, content_rect.height);
    surface.fill_rect(sidebar_rect, Color::new(0x0B, 0x10, 0x1C));
    surface.draw_vline(content_rect.x + sidebar_w as i32, content_rect.y, content_rect.height, Color::BORDER_SLATE);

    // Sidebar Category Title
    draw_string(surface, content_rect.x + 12, content_rect.y + 12, "PREFERENCES", Color::TEXT_MUTED, None, 1);

    // Sidebar Items
    let item_start_y = content_rect.y + 36;
    let item_h = 32i32;

    for (i, &sec) in SettingsSection::ALL.iter().enumerate() {
        let cur_y = item_start_y + (i as i32 * item_h);
        if cur_y + item_h > content_rect.bottom() {
            break;
        }

        let is_selected = app.current_section == sec;
        if is_selected {
            let active_rect = Rect::new(content_rect.x + 6, cur_y + 2, sidebar_w - 12, 28);
            surface.fill_rounded_rect(active_rect, 5, Color::new(0x1B, 0x25, 0x38));
            surface.draw_vline(content_rect.x + 8, cur_y + 6, 20, app.selected_accent.color());
        }

        let label_color = if is_selected {
            app.selected_accent.color()
        } else {
            Color::TEXT_LIGHT
        };
        let icon_tint = if is_selected {
            Some(app.selected_accent.color())
        } else {
            Some(Color::TEXT_MUTED)
        };

        draw_icon(surface, content_rect.x + 14, cur_y + 8, sec.icon(), icon_tint);
        draw_string(surface, content_rect.x + 36, cur_y + 10, sec.title(), label_color, None, 1);
    }

    // 3. Render Main Section Content Area
    let main_x = content_rect.x + sidebar_w as i32 + 1;
    let main_w = content_rect.width.saturating_sub(sidebar_w + 1);
    let main_rect = Rect::new(main_x, content_rect.y, main_w, content_rect.height);

    match app.current_section {
        SettingsSection::Appearance => render_appearance_section(surface, main_rect, &app),
        SettingsSection::Display => render_display_section(surface, main_rect, &app),
        SettingsSection::SystemInfo => render_system_section(surface, main_rect, &app),
        SettingsSection::Input => render_input_section(surface, main_rect, &app),
        SettingsSection::About => render_about_section(surface, main_rect, &app),
    }

    surface.set_clip_rect(saved_clip);
}

/// Renders the Appearance section.
fn render_appearance_section(surface: &mut Surface, area: Rect, app: &SettingsApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    // Header Card
    let header_rect = Rect::new(pad_x, cur_y, area.width.saturating_sub(28), 38);
    surface.fill_rounded_rect(header_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(header_rect, 6, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 12, cur_y + 8, "Theme: Cosmic Slate / Obsidian Azure", Color::WHITE, None, 1);
    draw_string(surface, pad_x + 12, cur_y + 22, "High contrast 60Hz double-buffered desktop theme", Color::TEXT_MUTED, None, 1);

    cur_y += 46;

    // Section 1: Accent Color Selector
    draw_string(surface, pad_x, cur_y, "ACCENT COLOUR", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let swatch_w = 72i32;
    let swatch_h = 28i32;
    let gap = 8i32;

    for (i, &acc) in AccentColorChoice::ALL.iter().enumerate() {
        let sw_x = pad_x + (i as i32 * (swatch_w + gap));
        let sw_rect = Rect::new(sw_x, cur_y, swatch_w as u32, swatch_h as u32);
        let is_selected = app.selected_accent == acc;

        surface.fill_rounded_rect(sw_rect, 5, Color::SURFACE_SLATE);
        let border_color = if is_selected { acc.color() } else { Color::BORDER_SLATE };
        surface.stroke_rounded_rect(sw_rect, 5, if is_selected { 2 } else { 1 }, border_color);

        // Color indicator pill
        let pill_rect = Rect::new(sw_x + 6, cur_y + 6, 16, 16);
        surface.fill_rounded_rect(pill_rect, 4, acc.color());

        if is_selected {
            draw_icon(surface, sw_x + 6, cur_y + 6, &ICON_CHECK, Some(Color::BLACK));
        }

        let text_color = if is_selected { Color::WHITE } else { Color::TEXT_LIGHT };
        draw_string(surface, sw_x + 26, cur_y + 9, acc.name(), text_color, None, 1);
    }

    cur_y += 38;

    // Section 2: Desktop Appearance Toggles
    draw_string(surface, pad_x, cur_y, "DESKTOP PREFERENCES", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let toggle_w = area.width.saturating_sub(28);
    let row_h = 28i32;

    // Toggle 1: Desktop Grid Pattern
    render_toggle_row(
        surface,
        Rect::new(pad_x, cur_y, toggle_w, row_h as u32),
        "Desktop Grid Pattern",
        app.desktop_grid_enabled,
        app.selected_accent.color(),
    );
    cur_y += 34;

    // Toggle 2: Window Transparency
    render_toggle_row(
        surface,
        Rect::new(pad_x, cur_y, toggle_w, row_h as u32),
        "Window Transparency",
        app.transparency_enabled,
        app.selected_accent.color(),
    );
    cur_y += 34;

    // Toggle 3: Smooth Animations
    render_toggle_row(
        surface,
        Rect::new(pad_x, cur_y, toggle_w, row_h as u32),
        "Smooth Animations",
        app.animations_enabled,
        app.selected_accent.color(),
    );
}

/// Helper to render a toggle switch row.
fn render_toggle_row(surface: &mut Surface, rect: Rect, label: &'static str, enabled: bool, accent: Color) {
    surface.fill_rounded_rect(rect, 5, Color::CARD_SLATE);
    surface.stroke_rounded_rect(rect, 5, 1, Color::BORDER_SLATE);

    draw_string(surface, rect.x + 12, rect.y + 9, label, Color::WHITE, None, 1);

    // Toggle switch on the right side
    let switch_w = 44i32;
    let switch_h = 20i32;
    let switch_x = rect.x + rect.width as i32 - switch_w - 10;
    let switch_y = rect.y + 4;
    let switch_rect = Rect::new(switch_x, switch_y, switch_w as u32, switch_h as u32);

    if enabled {
        surface.fill_rounded_rect(switch_rect, 10, accent);
        // Knob on right
        let knob_rect = Rect::new(switch_x + switch_w - 18, switch_y + 2, 16, 16);
        surface.fill_rounded_rect(knob_rect, 8, Color::WHITE);
        draw_string(surface, switch_x + 5, switch_y + 5, "ON", Color::BLACK, None, 1);
    } else {
        surface.fill_rounded_rect(switch_rect, 10, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(switch_rect, 10, 1, Color::BORDER_SLATE);
        // Knob on left
        let knob_rect = Rect::new(switch_x + 2, switch_y + 2, 16, 16);
        surface.fill_rounded_rect(knob_rect, 8, Color::TEXT_MUTED);
        draw_string(surface, switch_x + 20, switch_y + 5, "OFF", Color::TEXT_MUTED, None, 1);
    }
}

/// Renders the Display section.
fn render_display_section(surface: &mut Surface, area: Rect, app: &SettingsApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "HARDWARE DISPLAY ADAPTER", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Card 1: Framebuffer Resolution & Format
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Hardware Display Output",
        "1280 x 800 (Auto-detected GOP)",
        "Linear 32bpp Framebuffer | 5120 B/line Stride",
        app.selected_accent.color(),
    );
    cur_y += 54;

    // Card 2: Backbuffer Memory Allocation
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Double-Buffer Memory",
        "16,384 KiB (16 MiB Allocated)",
        "Direct-Mapped Pages (0x4444_5000_0000) | 1080p Safe",
        Color::SUCCESS_GREEN,
    );
    cur_y += 54;

    // Card 3: Display Timing & Refresh
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Refresh & Presentation",
        "60 Hz Target Refresh Rate",
        "Double-buffered tear-free surface swap",
        app.selected_accent.color(),
    );
    cur_y += 54;

    // Notice Box (Physical Mode Switching limitation)
    let notice_rect = Rect::new(pad_x, cur_y, card_w, 42);
    surface.fill_rounded_rect(notice_rect, 5, Color::new(0x1C, 0x18, 0x12));
    surface.stroke_rounded_rect(notice_rect, 5, 1, Color::WARNING_AMBER);
    draw_string(surface, pad_x + 10, cur_y + 8, "[NOTE] UEFI GOP Native Hardware Mode", Color::WARNING_AMBER, None, 1);
    draw_string(surface, pad_x + 10, cur_y + 22, "Physical mode-switching requires dedicated GPU PCI device drivers.", Color::TEXT_MUTED, None, 1);
}

/// Renders the System Information section.
fn render_system_section(surface: &mut Surface, area: Rect, app: &SettingsApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "SYSTEM SPECIFICATIONS", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Spec 1: Architecture
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 42),
        "CPU Architecture",
        "x86_64 Long Mode (64-bit)",
        "Ring 0 Kernel Execution | SSE2 Active",
        app.selected_accent.color(),
    );
    cur_y += 48;

    // Spec 2: Operating System & Kernel
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 42),
        "Kernel Version",
        "Micro-OS v0.11.17",
        "Pure Rust bare-metal kernel (#![no_std])",
        Color::ACCENT_CYAN,
    );
    cur_y += 48;

    // Spec 3: Memory Subsystems
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 42),
        "Physical & Virtual Memory",
        "497 MiB Usable RAM",
        "16 MiB Kernel Heap | 16 MiB Double-Buffer",
        Color::SUCCESS_GREEN,
    );
    cur_y += 48;

    // Spec 4: Timing & Multitasking
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 42),
        "Timer & Task Scheduling",
        "8254 PIT (100 Hz / 10 ms Quantum)",
        "Preemptive Round-Robin Multitasking via IRQ0",
        app.selected_accent.color(),
    );
}

/// Renders the Input section.
fn render_input_section(surface: &mut Surface, area: Rect, app: &SettingsApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "INPUT CONTROLLERS & DRIVERS (HP VICTUS 15)", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Driver 1: Keyboard
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 36),
        "Keyboard Driver",
        "Standard PS/2 Keyboard (Driver: Installed & Active)",
        "Ports 0x60 / 0x64 | Pure Polling | Scan Code Set 1",
        app.selected_accent.color(),
    );
    cur_y += 40;

    // Driver 2: Touchpad
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 36),
        "Touchpad Driver",
        "ELAN Input Device (I2C HID Probed | Pointer: Super+Arrows)",
        "Location: on I2C HID Device | AMD AMDI0010 | Manufacturer: ELAN",
        Color::ACCENT_AZURE,
    );
    cur_y += 40;

    // Driver 3: Wireless & Bluetooth
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 36),
        "Wireless & Bluetooth Drivers",
        "Realtek 8852BE-VT Wi-Fi 6 & Realtek Bluetooth Adapter",
        "PCIe 802.11ax & Bluetooth 5.2 HCI | Drivers: Included in ISO Image",
        Color::ACCENT_AZURE,
    );
    cur_y += 42;

    // Shortcuts Card
    let sc_rect = Rect::new(pad_x, cur_y, card_w, 106);
    surface.fill_rounded_rect(sc_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(sc_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 10, cur_y + 8, "SYSTEM KEYBOARD & POINTER SHORTCUTS", Color::WHITE, None, 1);
    let shortcuts = [
        ("Alt + Tab", "Cycle active window in Z-order"),
        ("Super + Arrows", "Move mouse cursor (Hardware pointer navigation)"),
        ("Super + Enter", "Left click at current mouse cursor position"),
        ("Super + M", "Minimize current focused window"),
        ("Up / Down", "Cycle settings sections & options"),
    ];

    let mut sc_y = cur_y + 24;
    for (key, desc) in shortcuts {
        draw_string(surface, pad_x + 10, sc_y, key, app.selected_accent.color(), None, 1);
        draw_string(surface, pad_x + 130, sc_y, desc, Color::TEXT_LIGHT, None, 1);
        sc_y += 14;
    }
}

/// Renders the About MicroOS section.
fn render_about_section(surface: &mut Surface, area: Rect, app: &SettingsApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    // Brand Title Card
    let brand_rect = Rect::new(pad_x, cur_y, area.width.saturating_sub(28), 64);
    surface.fill_rounded_rect(brand_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(brand_rect, 6, 1, app.selected_accent.color());

    draw_string(surface, pad_x + 14, cur_y + 10, "Micro-OS v0.11.17", Color::WHITE, None, 1);
    draw_string(surface, pad_x + 14, cur_y + 24, "Phase 4E Desktop Workstation Edition", app.selected_accent.color(), None, 1);

    // Badges
    let badges = ["[ Rust 2021 ]", "[ #![no_std] ]", "[ x86_64 ]", "[ Bare-Metal ]"];
    let mut badge_x = pad_x + 14;
    for badge in badges {
        draw_string(surface, badge_x, cur_y + 42, badge, Color::TEXT_MUTED, None, 1);
        badge_x += (badge.len() as i32 * 8) + 12;
    }

    cur_y += 74;

    // Overview Card
    let overview_rect = Rect::new(pad_x, cur_y, area.width.saturating_sub(28), 120);
    surface.fill_rounded_rect(overview_rect, 6, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(overview_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 10, "ARCHITECTURAL OVERVIEW", Color::WHITE, None, 1);
    let bullets = [
        "- Pure Rust kernel with zero C runtime or legacy dependencies",
        "- Double-buffered 2D graphics engine with sub-pixel clipping",
        "- Compositing Window Manager with Z-stacking & hit-testing",
        "- Interactive PS/2 keyboard & mouse input event dispatcher",
        "- In-memory Virtual File System (VFS) with hierarchical directories",
        "- Cosmic Slate / Obsidian Azure unified visual design system",
    ];

    let mut b_y = cur_y + 28;
    for bullet in bullets {
        draw_string(surface, pad_x + 12, b_y, bullet, Color::TEXT_LIGHT, None, 1);
        b_y += 14;
    }
}

/// Helper to render a labeled specification card with title, value, and description.
fn render_spec_card(
    surface: &mut Surface,
    rect: Rect,
    label: &'static str,
    value: &'static str,
    desc: &'static str,
    accent: Color,
) {
    surface.fill_rounded_rect(rect, 5, Color::CARD_SLATE);
    surface.stroke_rounded_rect(rect, 5, 1, Color::BORDER_SLATE);

    draw_string(surface, rect.x + 12, rect.y + 6, label, Color::TEXT_MUTED, None, 1);
    draw_string(surface, rect.x + 12, rect.y + 18, value, accent, None, 1);
    draw_string(surface, rect.x + 12, rect.y + 30, desc, Color::TEXT_LIGHT, None, 1);
}

/// Helper to open or bring to front the Settings application window.
pub fn open_or_focus_settings_window(wm: &mut WindowManager) {
    // 1. Check if a window titled "Settings" already exists
    let existing_id = wm.windows().iter().find(|w| w.title() == "Settings").map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Settings] Activated existing Settings window (ID: {:?})", id);
    } else {
        // 2. Spawn a new Settings window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 540u32.min(screen_w.saturating_sub(40));
        let win_h = 360u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 100i32;

        let settings_win = Window::new(
            WindowId::new(3),
            "Settings",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_GEAR)
        .with_min_size(440, 280)
        .with_content_renderer(render_settings_content);

        wm.add_window(settings_win);
        wm.set_active(WindowId::new(3));
        log::info!("[Settings] Spawned new Settings window (ID: 3) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global SettingsApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    SETTINGS_APP.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global SettingsApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    SETTINGS_APP.lock().handle_key(key)
}
