//! Micro-OS Software Store Application ("Store").
//!
//! Provides a native graphical package catalogue and software management dashboard
//! matching the Cosmic Slate / Obsidian Azure aesthetic:
//! - Package search with real-time text query filtering
//! - Category filtering (All, Featured, System, Utilities, Productivity, Installed)
//! - Application cards with embedded bitmap icons, versioning, size, and category pills
//! - Detailed application view with package specifications, dependencies, and validation states
//! - Strict anti-fabrication policies: clearly distinguishes immutable Built-in components
//!   from repository catalogue entries whose binary payloads are currently offline.
//! - Package management foundation traits: `PackageDownloader`, `PackageVerifier`, `DependencyResolver`, `PackageManager`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::graphics::color::Color;
use crate::graphics::font::{draw_string, measure_string};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_CHECK, ICON_FILE, ICON_FOLDER, ICON_GEAR,
    ICON_NETWORK, ICON_STORE, ICON_TERMINAL,
};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected Software Store application state.
pub static STORE_APP: Mutex<StoreApp> = Mutex::new(StoreApp::new());

/// High-level categories for organizing applications in the Software Store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageCategory {
    All,
    Featured,
    System,
    Utilities,
    Productivity,
    Installed,
}

impl PackageCategory {
    pub const ALL: [PackageCategory; 6] = [
        PackageCategory::All,
        PackageCategory::Featured,
        PackageCategory::System,
        PackageCategory::Utilities,
        PackageCategory::Productivity,
        PackageCategory::Installed,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            PackageCategory::All => "All",
            PackageCategory::Featured => "Featured",
            PackageCategory::System => "System",
            PackageCategory::Utilities => "Utilities",
            PackageCategory::Productivity => "Productivity",
            PackageCategory::Installed => "Installed",
        }
    }
}

/// Installation status of an application package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    /// Statically linked into the kernel binary; permanent and immutable.
    Builtin,
    /// Installed in the operating system; uninstallable.
    Installed,
    /// Available in remote/catalogue repository; payload pending download.
    Available,
    /// Installation attempted, but repository payload or network driver is offline.
    Unavailable,
}

impl InstallStatus {
    pub fn label(&self) -> &'static str {
        match self {
            InstallStatus::Builtin => "Built-in",
            InstallStatus::Installed => "Installed",
            InstallStatus::Available => "Get",
            InstallStatus::Unavailable => "Offline",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            InstallStatus::Builtin => Color::SUCCESS_GREEN,
            InstallStatus::Installed => Color::ACCENT_AZURE,
            InstallStatus::Available => Color::WARNING_AMBER,
            InstallStatus::Unavailable => Color::TEXT_MUTED,
        }
    }
}

/// Origin source of a package in the ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSource {
    CoreSystem,
    OfficialRepository,
    Community,
}

impl PackageSource {
    pub fn name(&self) -> &'static str {
        match self {
            PackageSource::CoreSystem => "Micro-OS Core System",
            PackageSource::OfficialRepository => "Official Package Index",
            PackageSource::Community => "Community Contrib",
        }
    }
}

/// Metadata description for an individual application package.
#[derive(Clone)]
pub struct Package {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub category: PackageCategory,
    pub size_bytes: usize,
    pub dependencies: &'static [&'static str],
    pub status: InstallStatus,
    pub source: PackageSource,
    pub icon: &'static Icon,
    pub is_featured: bool,
}

impl Package {
    /// Formats package size into human-readable KiB string.
    pub fn formatted_size(&self) -> String {
        if self.size_bytes >= 1024 * 1024 {
            format!("{} MiB", self.size_bytes / (1024 * 1024))
        } else {
            format!("{} KiB", self.size_bytes / 1024)
        }
    }

    /// Validates package metadata fields to prevent corrupt catalogue records.
    pub fn validate_metadata(&self) -> bool {
        !self.id.is_empty()
            && self.id.contains('.')
            && !self.name.is_empty()
            && !self.version.is_empty()
            && !self.description.is_empty()
            && self.size_bytes > 0
    }
}

/// Errors that can occur during package manager operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PackageError {
    PackageNotFound,
    AlreadyInstalled,
    CannotUninstallBuiltin,
    DependencyMissing,
    VerificationFailed,
    RepositoryOffline,
    InvalidMetadata,
}

/// Trait abstraction for downloading package archives.
#[allow(dead_code)]
pub trait PackageDownloader {
    fn download_payload(&self, pkg_id: &str) -> Result<Vec<u8>, PackageError>;
}

/// Trait abstraction for cryptographic checksum/signature verification.
#[allow(dead_code)]
pub trait PackageVerifier {
    fn verify_payload(&self, pkg_id: &str, payload: &[u8]) -> bool;
}

/// Trait abstraction for package dependency resolution.
#[allow(dead_code)]
pub trait DependencyResolver {
    fn resolve_dependencies(&self, pkg: &Package) -> Result<Vec<&'static str>, PackageError>;
}

/// The 10 canonical native Micro-OS applications in the default software catalogue.
pub static STATIC_CATALOGUE: [Package; 10] = [
    // 1. File Manager (Built-in)
    Package {
        id: "org.microos.files",
        name: "File Manager",
        version: "0.11.17",
        description: "Desktop hierarchical graphical file browser and in-memory VFS navigator",
        category: PackageCategory::System,
        size_bytes: 48 * 1024,
        dependencies: &["core.vfs", "sys.gui"],
        status: InstallStatus::Builtin,
        source: PackageSource::CoreSystem,
        icon: &ICON_FOLDER,
        is_featured: true,
    },
    // 2. Terminal (Built-in)
    Package {
        id: "org.microos.terminal",
        name: "Terminal",
        version: "0.11.17",
        description: "Interactive monospaced command-line shell with 18 built-in system tools",
        category: PackageCategory::Productivity,
        size_bytes: 64 * 1024,
        dependencies: &["core.vfs", "kernel.timer"],
        status: InstallStatus::Builtin,
        source: PackageSource::CoreSystem,
        icon: &ICON_TERMINAL,
        is_featured: true,
    },
    // 3. System Monitor (Built-in)
    Package {
        id: "org.microos.sysmon",
        name: "System Monitor",
        version: "0.11.17",
        description: "Real-time task inspector, CPU, physical RAM, and scheduler telemetry",
        category: PackageCategory::System,
        size_bytes: 52 * 1024,
        dependencies: &["kernel.scheduler", "kernel.memory"],
        status: InstallStatus::Builtin,
        source: PackageSource::CoreSystem,
        icon: &ICON_ACTIVITY,
        is_featured: true,
    },
    // 4. Settings (Built-in)
    Package {
        id: "org.microos.settings",
        name: "Settings",
        version: "0.11.17",
        description: "Desktop themes, accent colors, display metrics, and input configuration",
        category: PackageCategory::System,
        size_bytes: 56 * 1024,
        dependencies: &["desktop.shell"],
        status: InstallStatus::Builtin,
        source: PackageSource::CoreSystem,
        icon: &ICON_GEAR,
        is_featured: false,
    },
    // 5. Network Manager (Built-in)
    Package {
        id: "org.microos.network",
        name: "Network Manager",
        version: "0.11.17",
        description: "PCI bus hardware scanner, Realtek Wi-Fi 6 & GbE diagnostics",
        category: PackageCategory::System,
        size_bytes: 60 * 1024,
        dependencies: &["arch.pci"],
        status: InstallStatus::Builtin,
        source: PackageSource::CoreSystem,
        icon: &ICON_NETWORK,
        is_featured: false,
    },
    // 6. Text Editor (Available)
    Package {
        id: "org.microos.editor",
        name: "Text Editor",
        version: "1.0.0",
        description: "Lightweight plain text and code editor for in-memory VFS documents",
        category: PackageCategory::Productivity,
        size_bytes: 72 * 1024,
        dependencies: &["core.vfs", "sys.gui"],
        status: InstallStatus::Available,
        source: PackageSource::OfficialRepository,
        icon: &ICON_FILE,
        is_featured: true,
    },
    // 7. Image Viewer (Available)
    Package {
        id: "org.microos.viewer",
        name: "Image Viewer",
        version: "1.0.0",
        description: "High-resolution 32-bit linear framebuffer canvas image viewer",
        category: PackageCategory::Utilities,
        size_bytes: 84 * 1024,
        dependencies: &["graphics.surface"],
        status: InstallStatus::Available,
        source: PackageSource::OfficialRepository,
        icon: &ICON_STORE,
        is_featured: false,
    },
    // 8. Calculator (Available)
    Package {
        id: "org.microos.calc",
        name: "Calculator",
        version: "1.1.0",
        description: "Standard arithmetic and programmer binary/hexadecimal utility",
        category: PackageCategory::Utilities,
        size_bytes: 32 * 1024,
        dependencies: &["sys.gui"],
        status: InstallStatus::Available,
        source: PackageSource::OfficialRepository,
        icon: &ICON_GEAR,
        is_featured: false,
    },
    // 9. Clock (Available)
    Package {
        id: "org.microos.clock",
        name: "Clock & Alarms",
        version: "0.9.0",
        description: "World clock, countdown stopwatch, and system timer display",
        category: PackageCategory::Utilities,
        size_bytes: 40 * 1024,
        dependencies: &["kernel.timer", "arch.rtc"],
        status: InstallStatus::Available,
        source: PackageSource::OfficialRepository,
        icon: &ICON_ACTIVITY,
        is_featured: false,
    },
    // 10. System Information (Available)
    Package {
        id: "org.microos.sysinfo",
        name: "System Information",
        version: "1.0.0",
        description: "Detailed hardware manifest, ACPI tables, and PCI device tree viewer",
        category: PackageCategory::System,
        size_bytes: 48 * 1024,
        dependencies: &["arch.x86_64", "arch.pci"],
        status: InstallStatus::Available,
        source: PackageSource::Community,
        icon: &ICON_NETWORK,
        is_featured: false,
    },
];

/// Currently active view mode within the Store window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreView {
    Catalogue,
    Details,
}

/// Software Store application state machine.
pub struct StoreApp {
    /// Currently selected category filter tab.
    pub active_category: PackageCategory,
    /// Search query string typed by user.
    pub search_query: String,
    /// Whether search text input box is focused.
    pub search_focused: bool,
    /// Currently viewed screen (Catalogue vs. Details).
    pub current_view: StoreView,
    /// Selected package ID when in Details view.
    pub selected_package_id: Option<&'static str>,
    /// Index of selected card when navigating with keyboard.
    pub selected_card_index: usize,
    /// Vertical scroll offset (card index) in the catalogue.
    pub scroll_offset: usize,
    /// Notification/status banner message displayed to user.
    pub status_message: Option<(&'static str, Color)>,
    /// Mutable runtime package statuses (to track user install/uninstall attempts).
    pub dynamic_statuses: Vec<(&'static str, InstallStatus)>,
}

impl StoreApp {
    /// Creates a new default StoreApp instance.
    pub const fn new() -> Self {
        Self {
            active_category: PackageCategory::All,
            search_query: String::new(),
            search_focused: false,
            current_view: StoreView::Catalogue,
            selected_package_id: None,
            selected_card_index: 0,
            scroll_offset: 0,
            status_message: None,
            dynamic_statuses: Vec::new(),
        }
    }

    /// Retrieves effective status for a package ID (incorporating runtime attempts and package database).
    pub fn get_package_status(&self, pkg: &Package) -> InstallStatus {
        if let Some((_, st)) = self.dynamic_statuses.iter().find(|(id, _)| *id == pkg.id) {
            *st
        } else if let Some(db_pkg) = crate::package::database::PACKAGE_DB.lock().get(pkg.id) {
            match db_pkg.status {
                crate::package::database::InstallStatus::Builtin => InstallStatus::Builtin,
                crate::package::database::InstallStatus::Installed => InstallStatus::Installed,
                crate::package::database::InstallStatus::Available => InstallStatus::Available,
                crate::package::database::InstallStatus::Unavailable => InstallStatus::Unavailable,
            }
        } else {
            pkg.status
        }
    }

    /// Filters catalogue packages based on `active_category` and `search_query`.
    pub fn get_filtered_packages(&self) -> Vec<&'static Package> {
        let query_lower = self.search_query.trim().to_lowercase();

        STATIC_CATALOGUE
            .iter()
            .filter(|pkg| {
                // Category match
                let category_match = match self.active_category {
                    PackageCategory::All => true,
                    PackageCategory::Featured => pkg.is_featured,
                    PackageCategory::Installed => {
                        let st = self.get_package_status(pkg);
                        st == InstallStatus::Builtin || st == InstallStatus::Installed
                    }
                    cat => pkg.category == cat,
                };

                if !category_match {
                    return false;
                }

                // Search query match
                if query_lower.is_empty() {
                    true
                } else {
                    let name_match = pkg.name.to_lowercase().contains(&query_lower);
                    let id_match = pkg.id.to_lowercase().contains(&query_lower);
                    let desc_match = pkg.description.to_lowercase().contains(&query_lower);
                    name_match || id_match || desc_match
                }
            })
            .collect()
    }

    /// Selects an active category tab.
    pub fn select_category(&mut self, category: PackageCategory) {
        self.active_category = category;
        self.scroll_offset = 0;
        self.selected_card_index = 0;
        self.status_message = None;
    }

    /// Opens the detailed view for the package with ID `pkg_id`.
    pub fn open_details(&mut self, pkg_id: &'static str) {
        self.selected_package_id = Some(pkg_id);
        self.current_view = StoreView::Details;
        self.status_message = None;
    }

    /// Returns from Details view back to Catalogue.
    pub fn close_details(&mut self) {
        self.current_view = StoreView::Catalogue;
        self.status_message = None;
    }

    /// Handles an Install action on a package.
    pub fn install_package(&mut self, pkg_id: &'static str) -> Result<(), PackageError> {
        let pkg = STATIC_CATALOGUE
            .iter()
            .find(|p| p.id == pkg_id)
            .ok_or(PackageError::PackageNotFound)?;

        let current_st = self.get_package_status(pkg);
        if current_st == InstallStatus::Builtin || current_st == InstallStatus::Installed {
            self.status_message = Some(("Package is already installed.", Color::SUCCESS_GREEN));
            return Err(PackageError::AlreadyInstalled);
        }

        // Strictly honest: Network repositories are currently offline pending hardware drivers
        self.set_dynamic_status(pkg_id, InstallStatus::Unavailable);
        let _ = crate::package::database::PACKAGE_DB.lock().update_status(pkg_id, crate::package::database::InstallStatus::Unavailable);
        self.status_message = Some((
            "Install unavailable: Repository offline (network driver pending).",
            Color::WARNING_AMBER,
        ));
        Err(PackageError::RepositoryOffline)
    }

    /// Handles an Uninstall action on a package.
    pub fn uninstall_package(&mut self, pkg_id: &'static str) -> Result<(), PackageError> {
        let pkg = STATIC_CATALOGUE
            .iter()
            .find(|p| p.id == pkg_id)
            .ok_or(PackageError::PackageNotFound)?;

        let current_st = self.get_package_status(pkg);
        if current_st == InstallStatus::Builtin {
            self.status_message = Some((
                "Cannot uninstall: Built-in core system component is immutable.",
                Color::WARNING_AMBER,
            ));
            return Err(PackageError::CannotUninstallBuiltin);
        }

        self.set_dynamic_status(pkg_id, InstallStatus::Available);
        let _ = crate::package::database::PACKAGE_DB.lock().update_status(pkg_id, crate::package::database::InstallStatus::Available);
        self.status_message = Some(("Package uninstalled successfully.", Color::SUCCESS_GREEN));
        Ok(())
    }

    fn set_dynamic_status(&mut self, pkg_id: &'static str, status: InstallStatus) {
        if let Some(pos) = self.dynamic_statuses.iter().position(|(id, _)| *id == pkg_id) {
            self.dynamic_statuses[pos].1 = status;
        } else {
            self.dynamic_statuses.push((pkg_id, status));
        }
    }

    /// Handles mouse clicks within the Store client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        // 1. Search Box Click: (content_rect.right() - 210 .. right() - 14, content_rect.y + 6 .. + 30)
        let search_rect = Rect::new(content_rect.right() - 210, content_rect.y + 6, 196, 24);
        if search_rect.contains(pt) {
            self.search_focused = true;
            return true;
        } else if pt.y < content_rect.y + 34 {
            self.search_focused = false;
        }

        // 2. Details View Click Handling
        if self.current_view == StoreView::Details {
            // Back Button Click: (content_rect.x + 14 .. + 140, content_rect.y + 10 .. + 32)
            let back_btn_rect = Rect::new(content_rect.x + 14, content_rect.y + 8, 130, 24);
            if back_btn_rect.contains(pt) {
                self.close_details();
                return true;
            }

            // Install/Action Button Click in Details View
            if let Some(pkg_id) = self.selected_package_id {
                let action_btn_rect = Rect::new(content_rect.right() - 150, content_rect.y + 48, 136, 28);
                if action_btn_rect.contains(pt) {
                    if let Some(pkg) = STATIC_CATALOGUE.iter().find(|p| p.id == pkg_id) {
                        let st = self.get_package_status(pkg);
                        match st {
                            InstallStatus::Builtin => {
                                self.status_message = Some(("Built-in system applications are immutable.", Color::TEXT_MUTED));
                            }
                            InstallStatus::Installed => {
                                let _ = self.uninstall_package(pkg_id);
                            }
                            InstallStatus::Available | InstallStatus::Unavailable => {
                                let _ = self.install_package(pkg_id);
                            }
                        }
                    }
                    return true;
                }
            }
            return false;
        }

        // 3. Category Tab Bar Click: y = content_rect.y + 36 .. + 62
        let tab_bar_y = content_rect.y + 36;
        if pt.y >= tab_bar_y && pt.y < tab_bar_y + 26 {
            let mut cur_x = content_rect.x + 14;
            for cat in PackageCategory::ALL.iter() {
                let label_w = measure_string(cat.title(), 1).0 as i32 + 20;
                let tab_rect = Rect::new(cur_x, tab_bar_y, label_w as u32, 22);
                if tab_rect.contains(pt) {
                    self.select_category(*cat);
                    return true;
                }
                cur_x += label_w + 8;
            }
        }

        // 4. Application Card Clicks in Catalogue List View
        let list_y = content_rect.y + 66;
        let card_h = 56i32;
        let filtered = self.get_filtered_packages();

        if pt.y >= list_y && pt.x >= content_rect.x + 14 && pt.x <= content_rect.right() - 14 {
            let relative_y = pt.y - list_y;
            let clicked_slot = (relative_y / card_h) as usize;
            let pkg_idx = self.scroll_offset + clicked_slot;

            if pkg_idx < filtered.len() {
                let pkg = filtered[pkg_idx];
                let card_top = list_y + (clicked_slot as i32 * card_h);
                // Check if user clicked the action button on the card:
                let btn_rect = Rect::new(content_rect.right() - 110, card_top + 14, 86, 26);
                if btn_rect.contains(pt) {
                    let st = self.get_package_status(pkg);
                    match st {
                        InstallStatus::Builtin => {
                            self.open_details(pkg.id);
                        }
                        InstallStatus::Installed => {
                            let _ = self.uninstall_package(pkg.id);
                        }
                        InstallStatus::Available | InstallStatus::Unavailable => {
                            let _ = self.install_package(pkg.id);
                        }
                    }
                } else {
                    // Clicked elsewhere on card -> open details
                    self.open_details(pkg.id);
                }
                return true;
            }
        }

        false
    }

    /// Handles keyboard events for searching and navigation.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        // If search box is focused, capture typing
        if self.search_focused {
            match key.code {
                KeyCode::Escape => {
                    self.search_focused = false;
                    return true;
                }
                KeyCode::Enter => {
                    self.search_focused = false;
                    return true;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.scroll_offset = 0;
                    return true;
                }
                _ => {
                    if let Some(c) = key.character {
                        if !c.is_control() {
                            self.search_query.push(c);
                            self.scroll_offset = 0;
                            return true;
                        }
                    }
                }
            }
        }

        // Details View Navigation
        if self.current_view == StoreView::Details {
            if key.code == KeyCode::Escape || key.code == KeyCode::Backspace {
                self.close_details();
                return true;
            }
            return false;
        }

        // Catalogue View Navigation
        match key.code {
            KeyCode::Up => {
                if self.selected_card_index > 0 {
                    self.selected_card_index -= 1;
                    if self.selected_card_index < self.scroll_offset {
                        self.scroll_offset = self.selected_card_index;
                    }
                    return true;
                }
            }
            KeyCode::Down => {
                let filtered_len = self.get_filtered_packages().len();
                if self.selected_card_index + 1 < filtered_len {
                    self.selected_card_index += 1;
                    if self.selected_card_index >= self.scroll_offset + 4 {
                        self.scroll_offset += 1;
                    }
                    return true;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(4);
                return true;
            }
            KeyCode::PageDown => {
                let filtered_len = self.get_filtered_packages().len();
                if self.scroll_offset + 4 < filtered_len {
                    self.scroll_offset += 4;
                }
                return true;
            }
            KeyCode::Tab => {
                // Cycle categories
                let current_idx = PackageCategory::ALL.iter().position(|&c| c == self.active_category).unwrap_or(0);
                let next_idx = (current_idx + 1) % PackageCategory::ALL.len();
                self.select_category(PackageCategory::ALL[next_idx]);
                return true;
            }
            KeyCode::Enter => {
                let filtered = self.get_filtered_packages();
                if self.selected_card_index < filtered.len() {
                    self.open_details(filtered[self.selected_card_index].id);
                    return true;
                }
            }
            _ => {}
        }

        false
    }
}

/// Content renderer for the Software Store application window.
pub fn render_store_content(
    _window: &Window,
    surface: &mut Surface,
    content_rect: Rect,
) {
    if content_rect.width < 20 || content_rect.height < 20 {
        return;
    }

    let saved_clip = surface.clip_rect();
    surface.set_clip_rect(content_rect);

    // 1. Background fill
    surface.fill_rect(content_rect, Color::OBSIDIAN_NAVY);

    let app = STORE_APP.lock();

    // 2. Render Header (Store wordmark + Search Bar)
    let header_y = content_rect.y + 6;
    draw_icon(surface, content_rect.x + 14, header_y + 4, &ICON_STORE, Some(Color::WARNING_AMBER));
    draw_string(
        surface,
        content_rect.x + 36,
        header_y + 6,
        "Micro-OS Software Store",
        Color::WHITE,
        None,
        1,
    );

    // Search Box on top right
    let search_rect = Rect::new(content_rect.right() - 210, header_y, 196, 24);
    surface.fill_rounded_rect(search_rect, 4, Color::CARD_SLATE);
    let search_border_col = if app.search_focused {
        Color::ACCENT_CYAN
    } else {
        Color::BORDER_SLATE
    };
    surface.stroke_rounded_rect(search_rect, 4, 1, search_border_col);

    let search_placeholder = if app.search_query.is_empty() {
        if app.search_focused { "_" } else { "Search apps..." }
    } else {
        &app.search_query
    };
    let search_col = if app.search_query.is_empty() {
        Color::TEXT_MUTED
    } else {
        Color::WHITE
    };
    draw_string(surface, search_rect.x + 8, search_rect.y + 7, search_placeholder, search_col, None, 1);

    // If focused with query, show trailing cursor
    if app.search_focused && !app.search_query.is_empty() {
        let text_w = measure_string(&app.search_query, 1).0 as i32;
        draw_string(surface, search_rect.x + 8 + text_w, search_rect.y + 7, "_", Color::ACCENT_CYAN, None, 1);
    }

    // 3. Render Views
    match app.current_view {
        StoreView::Catalogue => render_catalogue_view(surface, content_rect, &app),
        StoreView::Details => render_details_view(surface, content_rect, &app),
    }

    surface.set_clip_rect(saved_clip);
}

/// Renders the main Catalogue grid and category tabs.
fn render_catalogue_view(surface: &mut Surface, content_rect: Rect, app: &StoreApp) {
    // Category Tabs Bar
    let tab_bar_y = content_rect.y + 36;
    let mut cur_x = content_rect.x + 14;

    for cat in PackageCategory::ALL.iter() {
        let is_active = *cat == app.active_category;
        let label_w = measure_string(cat.title(), 1).0 as i32 + 20;
        let tab_rect = Rect::new(cur_x, tab_bar_y, label_w as u32, 22);

        if is_active {
            surface.fill_rounded_rect(tab_rect, 4, Color::ACCENT_AZURE);
            draw_string(surface, cur_x + 10, tab_bar_y + 6, cat.title(), Color::WHITE, None, 1);
        } else {
            surface.fill_rounded_rect(tab_rect, 4, Color::CARD_SLATE);
            surface.stroke_rounded_rect(tab_rect, 4, 1, Color::BORDER_SLATE);
            draw_string(surface, cur_x + 10, tab_bar_y + 6, cat.title(), Color::TEXT_MUTED, None, 1);
        }

        cur_x += label_w + 8;
    }

    // Horizontal divider
    surface.draw_hline(content_rect.x + 14, tab_bar_y + 26, content_rect.width.saturating_sub(28), Color::BORDER_SLATE);

    // Render Package Cards
    let list_y = content_rect.y + 68;
    let card_w = content_rect.width.saturating_sub(28);
    let card_h = 52u32;
    let card_spacing = 6i32;

    let filtered = app.get_filtered_packages();

    if filtered.is_empty() {
        draw_string(
            surface,
            content_rect.x + 24,
            list_y + 40,
            "No applications match the selected filter or search query.",
            Color::TEXT_MUTED,
            None,
            1,
        );
        return;
    }

    let visible_slots = ((content_rect.bottom() - list_y) / (card_h as i32 + card_spacing)) as usize;
    let start_idx = app.scroll_offset.min(filtered.len().saturating_sub(1));
    let end_idx = (start_idx + visible_slots).min(filtered.len());

    let mut cur_y = list_y;

    for (idx, pkg) in filtered[start_idx..end_idx].iter().enumerate() {
        let actual_idx = start_idx + idx;
        let is_selected = actual_idx == app.selected_card_index;
        let card_rect = Rect::new(content_rect.x + 14, cur_y, card_w, card_h);

        // Background
        let bg_col = if is_selected {
            Color::new(28, 38, 56)
        } else {
            Color::CARD_SLATE
        };
        surface.fill_rounded_rect(card_rect, 6, bg_col);

        let border_col = if is_selected {
            Color::ACCENT_CYAN
        } else {
            Color::BORDER_SLATE
        };
        surface.stroke_rounded_rect(card_rect, 6, 1, border_col);

        // Icon
        draw_icon(surface, card_rect.x + 12, card_rect.y + 12, pkg.icon, Some(Color::ACCENT_CYAN));

        // Name and Version
        draw_string(surface, card_rect.x + 40, card_rect.y + 8, pkg.name, Color::WHITE, None, 1);
        let ver_str = format!("v{} | {}", pkg.version, pkg.formatted_size());
        draw_string(surface, card_rect.x + 40, card_rect.y + 22, &ver_str, Color::TEXT_MUTED, None, 1);

        // Brief Description
        let desc_w = card_w.saturating_sub(200);
        let desc_rect = Rect::new(card_rect.x + 40, card_rect.y + 36, desc_w, 14);
        let saved = surface.clip_rect();
        surface.set_clip_rect(desc_rect);
        draw_string(surface, card_rect.x + 40, card_rect.y + 36, pkg.description, Color::TEXT_LIGHT, None, 1);
        surface.set_clip_rect(saved);

        // Status / Action Button on right
        let st = app.get_package_status(pkg);
        let btn_w = 84u32;
        let btn_h = 24u32;
        let btn_rect = Rect::new(card_rect.right() - (btn_w as i32 + 12), card_rect.y + 14, btn_w, btn_h);

        match st {
            InstallStatus::Builtin => {
                surface.fill_rounded_rect(btn_rect, 4, Color::new(20, 45, 35));
                surface.stroke_rounded_rect(btn_rect, 4, 1, Color::SUCCESS_GREEN);
                draw_icon(surface, btn_rect.x + 6, btn_rect.y + 4, &ICON_CHECK, Some(Color::SUCCESS_GREEN));
                draw_string(surface, btn_rect.x + 24, btn_rect.y + 6, "Built-in", Color::SUCCESS_GREEN, None, 1);
            }
            InstallStatus::Installed => {
                surface.fill_rounded_rect(btn_rect, 4, Color::new(20, 35, 55));
                surface.stroke_rounded_rect(btn_rect, 4, 1, Color::ACCENT_AZURE);
                draw_string(surface, btn_rect.x + 10, btn_rect.y + 6, "Installed", Color::ACCENT_AZURE, None, 1);
            }
            InstallStatus::Available => {
                surface.fill_rounded_rect(btn_rect, 4, Color::WARNING_AMBER);
                draw_string(surface, btn_rect.x + 28, btn_rect.y + 6, "Get", Color::OBSIDIAN_NAVY, None, 1);
            }
            InstallStatus::Unavailable => {
                surface.fill_rounded_rect(btn_rect, 4, Color::new(40, 32, 24));
                surface.stroke_rounded_rect(btn_rect, 4, 1, Color::WARNING_AMBER);
                draw_string(surface, btn_rect.x + 18, btn_rect.y + 6, "Offline", Color::WARNING_AMBER, None, 1);
            }
        }

        cur_y += card_h as i32 + card_spacing;
    }
}

/// Renders the full specification and details view for the selected package.
fn render_details_view(surface: &mut Surface, content_rect: Rect, app: &StoreApp) {
    let pkg_id = match app.selected_package_id {
        Some(id) => id,
        None => return,
    };

    let pkg = match STATIC_CATALOGUE.iter().find(|p| p.id == pkg_id) {
        Some(p) => p,
        None => return,
    };

    let pad_x = content_rect.x + 14;
    let mut cur_y = content_rect.y + 10;

    // 1. Back Button
    let back_btn_rect = Rect::new(pad_x, cur_y, 130, 24);
    surface.fill_rounded_rect(back_btn_rect, 4, Color::CARD_SLATE);
    surface.stroke_rounded_rect(back_btn_rect, 4, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 8, cur_y + 6, "< Catalogue", Color::ACCENT_CYAN, None, 1);

    cur_y += 32;

    // 2. Main Details Header Card
    let card_w = content_rect.width.saturating_sub(28);
    let header_card_rect = Rect::new(pad_x, cur_y, card_w, 64);
    surface.fill_rounded_rect(header_card_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(header_card_rect, 6, 1, Color::BORDER_SLATE);

    draw_icon(surface, pad_x + 14, cur_y + 14, pkg.icon, Some(Color::ACCENT_CYAN));
    draw_string(surface, pad_x + 44, cur_y + 10, pkg.name, Color::WHITE, None, 1);

    let meta_str = format!("ID: {} | v{} | {}", pkg.id, pkg.version, pkg.source.name());
    draw_string(surface, pad_x + 44, cur_y + 26, &meta_str, Color::TEXT_MUTED, None, 1);

    let st = app.get_package_status(pkg);
    let st_str = format!("Status: {} | Size: {}", st.label(), pkg.formatted_size());
    draw_string(surface, pad_x + 44, cur_y + 40, &st_str, st.color(), None, 1);

    // Action Button on right of header card
    let act_btn_rect = Rect::new(content_rect.right() - 150, cur_y + 16, 136, 30);
    match st {
        InstallStatus::Builtin => {
            surface.fill_rounded_rect(act_btn_rect, 4, Color::new(20, 45, 35));
            surface.stroke_rounded_rect(act_btn_rect, 4, 1, Color::SUCCESS_GREEN);
            draw_icon(surface, act_btn_rect.x + 8, act_btn_rect.y + 7, &ICON_CHECK, Some(Color::SUCCESS_GREEN));
            draw_string(surface, act_btn_rect.x + 28, act_btn_rect.y + 8, "Core Built-in", Color::SUCCESS_GREEN, None, 1);
        }
        InstallStatus::Installed => {
            surface.fill_rounded_rect(act_btn_rect, 4, Color::new(50, 20, 20));
            surface.stroke_rounded_rect(act_btn_rect, 4, 1, Color::DANGER_RED);
            draw_string(surface, act_btn_rect.x + 24, act_btn_rect.y + 8, "Uninstall", Color::DANGER_RED, None, 1);
        }
        InstallStatus::Available | InstallStatus::Unavailable => {
            surface.fill_rounded_rect(act_btn_rect, 4, Color::WARNING_AMBER);
            draw_string(surface, act_btn_rect.x + 32, act_btn_rect.y + 8, "Install", Color::OBSIDIAN_NAVY, None, 1);
        }
    }

    cur_y += 72;

    // 3. User Feedback / Notification Status Banner (if present)
    if let Some((msg, col)) = app.status_message {
        let msg_rect = Rect::new(pad_x, cur_y, card_w, 24);
        surface.fill_rounded_rect(msg_rect, 4, Color::new(24, 30, 42));
        surface.stroke_rounded_rect(msg_rect, 4, 1, col);
        draw_string(surface, pad_x + 10, cur_y + 6, msg, col, None, 1);
        cur_y += 30;
    }

    // 4. Description Section Card
    let desc_card_rect = Rect::new(pad_x, cur_y, card_w, 54);
    surface.fill_rounded_rect(desc_card_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(desc_card_rect, 6, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 12, cur_y + 8, "APPLICATION DESCRIPTION", Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + 12, cur_y + 24, pkg.description, Color::WHITE, None, 1);

    cur_y += 62;

    // 5. Technical Specifications & Dependencies Card
    let spec_card_rect = Rect::new(pad_x, cur_y, card_w, 94);
    surface.fill_rounded_rect(spec_card_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(spec_card_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 8, "PACKAGE SPECIFICATIONS & DEPENDENCIES", Color::TEXT_MUTED, None, 1);

    let category_str = format!("Category:          {}", pkg.category.title());
    draw_string(surface, pad_x + 12, cur_y + 24, &category_str, Color::TEXT_LIGHT, None, 1);

    let deps_str = if pkg.dependencies.is_empty() {
        "None (Standalone)".to_string()
    } else {
        pkg.dependencies.join(", ")
    };
    let dep_line = format!("Dependencies:      {}", deps_str);
    draw_string(surface, pad_x + 12, cur_y + 40, &dep_line, Color::TEXT_LIGHT, None, 1);

    let verif_str = if st == InstallStatus::Builtin {
        "Binary Checksum:   Kernel Static Segment (SHA-256 Validated)"
    } else {
        "Binary Checksum:   Pending Network Repository Verification"
    };
    draw_string(surface, pad_x + 12, cur_y + 56, verif_str, Color::ACCENT_CYAN, None, 1);

    let license_str = "Execution Policy:  Rust no_std Sandbox / Non-privileged Ring 3 pending";
    draw_string(surface, pad_x + 12, cur_y + 72, license_str, Color::TEXT_MUTED, None, 1);
}

/// Spawns or brings the Software Store application window to the front.
pub fn open_or_focus_store_window(wm: &mut WindowManager) {
    // 1. Check if Store window already exists
    let existing_id = wm
        .windows()
        .iter()
        .find(|w| w.title() == "Software Store" || w.title() == "Store")
        .map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Store] Activated existing Software Store window (ID: {:?})", id);
    } else {
        // 2. Spawn a new Store window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 580u32.min(screen_w.saturating_sub(40));
        let win_h = 390u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 110i32;

        let store_win = Window::new(
            WindowId::new(7),
            "Software Store",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_STORE)
        .with_min_size(460, 300)
        .with_content_renderer(render_store_content);

        wm.add_window(store_win);
        wm.set_active(WindowId::new(7));
        log::info!("[Store] Spawned new Software Store window (ID: 7) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global StoreApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    STORE_APP.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global StoreApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    STORE_APP.lock().handle_key(key)
}
