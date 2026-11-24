//! Micro-OS File Manager Application ("Files").
//!
//! Provides a desktop graphical file manager featuring a navigation toolbar
//! (Back, Forward, Up), an interactive breadcrumb address bar, quick-access sidebar
//! shortcuts (Home, Documents, Downloads, Pictures, Music, Videos), file/folder
//! grid rendering with embedded bitmap icons, and complete keyboard/mouse navigation.
//!
//! Note: Backed by the in-memory Virtual File System (VFS).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::fs::vfs::{FsEntry, FsNodeKind, Vfs, VFS};
use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{draw_icon, Icon, ICON_FILE, ICON_FOLDER};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected File Manager application state.
pub static FILE_MANAGER: Mutex<FileManagerApp> = Mutex::new(FileManagerApp::new());

/// Sidebar shortcut entry.
pub struct SidebarShortcut {
    pub label: &'static str,
    pub path: &'static str,
}

/// The 6 canonical sidebar navigation shortcuts for Micro-OS.
pub static SIDEBAR_SHORTCUTS: [SidebarShortcut; 6] = [
    SidebarShortcut { label: "Home", path: "/home/user" },
    SidebarShortcut { label: "Documents", path: "/home/user/Documents" },
    SidebarShortcut { label: "Downloads", path: "/home/user/Downloads" },
    SidebarShortcut { label: "Pictures", path: "/home/user/Pictures" },
    SidebarShortcut { label: "Music", path: "/home/user/Music" },
    SidebarShortcut { label: "Videos", path: "/home/user/Videos" },
];

/// File Manager Application state machine.
pub struct FileManagerApp {
    /// Currently displayed directory path.
    pub current_path: String,
    /// Stack of previously visited paths for Back navigation.
    pub back_history: Vec<String>,
    /// Stack of paths for Forward navigation.
    pub forward_history: Vec<String>,
    /// Index of currently selected item in the main file list.
    pub selected_index: Option<usize>,
}

impl FileManagerApp {
    /// Creates a new File Manager instance rooted at `/home/user`.
    pub const fn new() -> Self {
        Self {
            current_path: String::new(),
            back_history: Vec::new(),
            forward_history: Vec::new(),
            selected_index: None,
        }
    }

    /// Initializes default path to `/home/user` if uninitialized.
    pub fn ensure_initialized(&mut self) {
        if self.current_path.is_empty() {
            self.current_path = "/home/user".to_string();
        }
    }

    /// Navigates to the specified target path.
    ///
    /// Validates directory existence in VFS before transitioning.
    /// Returns `true` if path changed.
    pub fn navigate_to(&mut self, path: &str) -> bool {
        let canonical = Vfs::canonicalize_path(path);
        if canonical == self.current_path {
            return false;
        }

        // Validate that target path is an existing directory
        if !VFS.lock().is_dir(&canonical) {
            log::warn!("[Files] Cannot navigate: '{}' is not an existing directory", canonical);
            return false;
        }

        if !self.current_path.is_empty() {
            self.back_history.push(self.current_path.clone());
        }
        self.forward_history.clear();
        self.current_path = canonical;
        self.selected_index = None;
        true
    }

    /// Navigates back in history.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.back_history.pop() {
            self.forward_history.push(self.current_path.clone());
            self.current_path = prev;
            self.selected_index = None;
            true
        } else {
            false
        }
    }

    /// Navigates forward in history.
    pub fn go_forward(&mut self) -> bool {
        if let Some(next) = self.forward_history.pop() {
            self.back_history.push(self.current_path.clone());
            self.current_path = next;
            self.selected_index = None;
            true
        } else {
            false
        }
    }

    /// Navigates up to parent directory.
    pub fn go_up(&mut self) -> bool {
        if self.current_path == "/" {
            return false;
        }

        let segments = Vfs::parse_segments(&self.current_path);
        if segments.len() <= 1 {
            self.navigate_to("/")
        } else {
            let mut parent = String::new();
            for seg in &segments[..segments.len() - 1] {
                parent.push('/');
                parent.push_str(seg);
            }
            self.navigate_to(&parent)
        }
    }

    /// Returns whether the Back button is enabled.
    #[inline(always)]
    pub fn can_go_back(&self) -> bool {
        !self.back_history.is_empty()
    }

    /// Returns whether the Forward button is enabled.
    #[inline(always)]
    pub fn can_go_forward(&self) -> bool {
        !self.forward_history.is_empty()
    }

    /// Returns current entries in the active directory.
    pub fn current_entries(&self) -> Vec<FsEntry> {
        VFS.lock()
            .list_dir(&self.current_path)
            .unwrap_or_else(|_| Vec::new())
    }

    /// Handles mouse clicks within the window client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        self.ensure_initialized();

        // 1. Toolbar clicks (y: content_rect.y .. content_rect.y + 36)
        if pt.y >= content_rect.y && pt.y < content_rect.y + 36 {
            let btn_y = content_rect.y + 6;
            // Back button [ < ] (x: content_rect.x + 8 .. + 34)
            if pt.x >= content_rect.x + 8 && pt.x < content_rect.x + 34 && pt.y >= btn_y && pt.y < btn_y + 24 {
                return self.go_back();
            }
            // Forward button [ > ] (x: content_rect.x + 38 .. + 64)
            if pt.x >= content_rect.x + 38 && pt.x < content_rect.x + 64 && pt.y >= btn_y && pt.y < btn_y + 24 {
                return self.go_forward();
            }
            // Up button [ ^ ] (x: content_rect.x + 68 .. + 94)
            if pt.x >= content_rect.x + 68 && pt.x < content_rect.x + 94 && pt.y >= btn_y && pt.y < btn_y + 24 {
                return self.go_up();
            }
            return false;
        }

        let sidebar_w = 125i32;

        // 2. Sidebar clicks (x: content_rect.x .. content_rect.x + 125, y >= content_rect.y + 36)
        if pt.x >= content_rect.x && pt.x < content_rect.x + sidebar_w && pt.y >= content_rect.y + 36 {
            let start_y = content_rect.y + 36 + 22;
            let item_h = 26i32;

            for (i, shortcut) in SIDEBAR_SHORTCUTS.iter().enumerate() {
                let cur_y = start_y + (i as i32 * item_h);
                if pt.y >= cur_y && pt.y < cur_y + item_h {
                    return self.navigate_to(shortcut.path);
                }
            }
            return false;
        }

        // 3. Main file-view area clicks (x > content_rect.x + 125, y >= content_rect.y + 36)
        if pt.x >= content_rect.x + sidebar_w && pt.y >= content_rect.y + 36 + 22 {
            let list_y = content_rect.y + 36 + 22;
            let row_h = 26i32;
            let row_idx = ((pt.y - list_y) / row_h) as usize;

            let entries = self.current_entries();
            if row_idx < entries.len() {
                let entry = &entries[row_idx];
                if entry.kind == FsNodeKind::Directory {
                    // Clicking a directory navigates directly into it
                    let new_path = if self.current_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", self.current_path, entry.name)
                    };
                    return self.navigate_to(&new_path);
                } else {
                    // Clicking a file selects it
                    self.selected_index = Some(row_idx);
                    return true;
                }
            }
        }

        false
    }

    /// Handles keyboard navigation events.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.ensure_initialized();

        if key.action != KeyAction::Pressed {
            return false;
        }

        let entries = self.current_entries();

        match key.code {
            KeyCode::Backspace => {
                // Backspace navigates to parent directory or back
                if !self.go_back() {
                    self.go_up()
                } else {
                    true
                }
            }
            KeyCode::Down => {
                if entries.is_empty() {
                    return false;
                }
                let next = match self.selected_index {
                    Some(i) => (i + 1).min(entries.len() - 1),
                    None => 0,
                };
                self.selected_index = Some(next);
                true
            }
            KeyCode::Up => {
                if entries.is_empty() {
                    return false;
                }
                let prev = match self.selected_index {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.selected_index = Some(prev);
                true
            }
            KeyCode::Enter => {
                if let Some(idx) = self.selected_index {
                    if idx < entries.len() {
                        let entry = &entries[idx];
                        if entry.kind == FsNodeKind::Directory {
                            let new_path = if self.current_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", self.current_path, entry.name)
                            };
                            return self.navigate_to(&new_path);
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

/// Global client content renderer for the Files window.
pub fn render_files_content(_window: &Window, surface: &mut Surface, content_rect: Rect) {
    let mut app = FILE_MANAGER.lock();
    app.ensure_initialized();

    let saved_clip = surface.clip_rect();
    let client_clip = match saved_clip.intersect(&content_rect) {
        Some(c) => c,
        None => return,
    };
    surface.set_clip_rect(client_clip);

    // 1. Fill client background (Deep Obsidian Slate)
    surface.fill_rect(content_rect, Color::new(0x0E, 0x14, 0x24));

    // 2. Render Top Toolbar (Height: 36 px)
    let toolbar_y = content_rect.y;
    let toolbar_rect = Rect::new(content_rect.x, toolbar_y, content_rect.width, 36);
    surface.fill_rect(toolbar_rect, Color::CARD_SLATE);
    surface.draw_hline(content_rect.x, toolbar_y + 35, content_rect.width, Color::BORDER_SLATE);

    // Navigation buttons: Back [<], Forward [>], Up [^]
    let btn_y = toolbar_y + 6;
    let btn_bg = Color::SURFACE_SLATE;

    // Back Button
    let back_rect = Rect::new(content_rect.x + 8, btn_y, 26, 24);
    surface.fill_rounded_rect(back_rect, 4, btn_bg);
    let back_fg = if app.can_go_back() { Color::WHITE } else { Color::TEXT_MUTED };
    surface.stroke_rounded_rect(back_rect, 4, 1, if app.can_go_back() { Color::ACCENT_CYAN } else { Color::BORDER_SLATE });
    draw_string(surface, back_rect.x + 9, back_rect.y + 4, "<", back_fg, None, 1);

    // Forward Button
    let fwd_rect = Rect::new(content_rect.x + 38, btn_y, 26, 24);
    surface.fill_rounded_rect(fwd_rect, 4, btn_bg);
    let fwd_fg = if app.can_go_forward() { Color::WHITE } else { Color::TEXT_MUTED };
    surface.stroke_rounded_rect(fwd_rect, 4, 1, if app.can_go_forward() { Color::ACCENT_CYAN } else { Color::BORDER_SLATE });
    draw_string(surface, fwd_rect.x + 9, fwd_rect.y + 4, ">", fwd_fg, None, 1);

    // Up Button
    let up_rect = Rect::new(content_rect.x + 68, btn_y, 26, 24);
    surface.fill_rounded_rect(up_rect, 4, btn_bg);
    let up_fg = if app.current_path != "/" { Color::WHITE } else { Color::TEXT_MUTED };
    surface.stroke_rounded_rect(up_rect, 4, 1, if app.current_path != "/" { Color::ACCENT_CYAN } else { Color::BORDER_SLATE });
    draw_string(surface, up_rect.x + 9, up_rect.y + 4, "^", up_fg, None, 1);

    // Path / Address Bar Pill
    let path_x = content_rect.x + 100;
    let path_w = content_rect.width.saturating_sub(108);
    let path_rect = Rect::new(path_x, btn_y, path_w, 24);
    surface.fill_rounded_rect(path_rect, 6, Color::new(0x0A, 0x0E, 0x17));
    surface.stroke_rounded_rect(path_rect, 6, 1, Color::BORDER_SLATE);

    // Draw path string inside address pill (clipped to pill)
    if let Some(path_clip) = surface.clip_rect().intersect(&Rect::new(path_x + 6, btn_y, path_w.saturating_sub(12), 24)) {
        surface.set_clip_rect(path_clip);
        draw_string(surface, path_x + 8, btn_y + 4, &app.current_path, Color::ACCENT_CYAN, None, 1);
        surface.set_clip_rect(client_clip);
    }

    // 3. Render Left Sidebar (Width: 125 px)
    let sidebar_w = 125u32;
    let sidebar_y = content_rect.y + 36;
    let sidebar_h = content_rect.height.saturating_sub(36);
    let sidebar_rect = Rect::new(content_rect.x, sidebar_y, sidebar_w, sidebar_h);
    surface.fill_rect(sidebar_rect, Color::new(0x0B, 0x10, 0x1C));
    surface.draw_vline(content_rect.x + sidebar_w as i32, sidebar_y, sidebar_h, Color::BORDER_SLATE);

    // Sidebar Category Title
    draw_string(surface, content_rect.x + 10, sidebar_y + 8, "PLACES", Color::TEXT_MUTED, None, 1);

    // Sidebar Shortcuts
    let shortcut_start_y = sidebar_y + 22;
    let shortcut_h = 26i32;

    for (i, shortcut) in SIDEBAR_SHORTCUTS.iter().enumerate() {
        let cur_y = shortcut_start_y + (i as i32 * shortcut_h);
        if cur_y + shortcut_h > content_rect.bottom() {
            break;
        }

        let is_active = app.current_path == shortcut.path;
        if is_active {
            let active_rect = Rect::new(content_rect.x + 6, cur_y + 2, sidebar_w - 12, 22);
            surface.fill_rounded_rect(active_rect, 4, Color::new(0x1B, 0x25, 0x38));
            surface.draw_vline(content_rect.x + 8, cur_y + 4, 18, Color::ACCENT_CYAN);
        }

        let label_color = if is_active { Color::ACCENT_CYAN } else { Color::TEXT_LIGHT };
        draw_icon(surface, content_rect.x + 12, cur_y + 5, &ICON_FOLDER, Some(if is_active { Color::ACCENT_CYAN } else { Color::TEXT_MUTED }));
        draw_string(surface, content_rect.x + 32, cur_y + 7, shortcut.label, label_color, None, 1);
    }

    // 4. Render Main File-View Area (Right of Sidebar)
    let file_view_x = content_rect.x + sidebar_w as i32 + 1;
    let file_view_y = sidebar_y;
    let file_view_w = content_rect.width.saturating_sub(sidebar_w + 1);
    let file_view_h = sidebar_h;

    // Table Header Row (Name & Size)
    let header_rect = Rect::new(file_view_x, file_view_y, file_view_w, 20);
    surface.fill_rect(header_rect, Color::new(0x10, 0x17, 0x27));
    surface.draw_hline(file_view_x, file_view_y + 19, file_view_w, Color::BORDER_SLATE);
    draw_string(surface, file_view_x + 36, file_view_y + 6, "NAME", Color::TEXT_MUTED, None, 1);
    draw_string(surface, file_view_x + file_view_w as i32 - 70, file_view_y + 6, "SIZE", Color::TEXT_MUTED, None, 1);

    // File / Directory Listing
    let entries = app.current_entries();
    let list_start_y = file_view_y + 22;
    let row_h = 26i32;

    if entries.is_empty() {
        // Display friendly empty directory indicator
        let empty_msg = "[ Empty Directory ]";
        let sub_msg = "Use VFS create/mkdir to add items";
        let center_x = file_view_x + (file_view_w as i32 - (empty_msg.len() as i32 * 8)) / 2;
        let center_y = file_view_y + (file_view_h as i32 / 2) - 10;
        draw_string(surface, center_x, center_y, empty_msg, Color::TEXT_MUTED, None, 1);
        let sub_x = file_view_x + (file_view_w as i32 - (sub_msg.len() as i32 * 8)) / 2;
        draw_string(surface, sub_x, center_y + 16, sub_msg, Color::rgba(0x94, 0xA3, 0xB8, 120), None, 1);
    } else {
        for (i, entry) in entries.iter().enumerate() {
            let row_y = list_start_y + (i as i32 * row_h);
            if row_y + row_h > content_rect.bottom() {
                break;
            }

            let is_selected = app.selected_index == Some(i);
            if is_selected {
                let row_rect = Rect::new(file_view_x + 4, row_y + 1, file_view_w.saturating_sub(8), 24);
                surface.fill_rounded_rect(row_rect, 4, Color::new(0x1E, 0x2B, 0x42));
                surface.stroke_rounded_rect(row_rect, 4, 1, Color::ACCENT_AZURE);
            }

            // Draw Icon
            let icon: &Icon = match entry.kind {
                FsNodeKind::Directory => &ICON_FOLDER,
                FsNodeKind::File => &ICON_FILE,
            };
            let icon_tint = match entry.kind {
                FsNodeKind::Directory => Some(Color::ACCENT_AZURE),
                FsNodeKind::File => Some(Color::TEXT_LIGHT),
            };
            draw_icon(surface, file_view_x + 12, row_y + 5, icon, icon_tint);

            // Draw Name
            let name_color = if is_selected { Color::ACCENT_CYAN } else { Color::WHITE };
            draw_string(surface, file_view_x + 36, row_y + 9, &entry.name, name_color, None, 1);

            // Draw Size or Folder label
            let size_str = match entry.kind {
                FsNodeKind::Directory => "Folder".to_string(),
                FsNodeKind::File => format!("{} B", entry.size),
            };
            draw_string(
                surface,
                file_view_x + file_view_w as i32 - 70,
                row_y + 9,
                &size_str,
                Color::TEXT_MUTED,
                None,
                1,
            );
        }
    }

    surface.set_clip_rect(saved_clip);
}

/// Helper to open or bring to front the Files application window.
pub fn open_or_focus_files_window(wm: &mut WindowManager) {
    // 1. Check if a window titled "Files" already exists
    let existing_id = wm.windows().iter().find(|w| w.title() == "Files").map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Files] Activated existing Files window (ID: {:?})", id);
    } else {
        // 2. Spawn a new Files window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 560u32.min(screen_w.saturating_sub(40));
        let win_h = 350u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 80i32;

        let files_win = Window::new(
            WindowId::new(4),
            "Files",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_FOLDER)
        .with_min_size(440, 260)
        .with_content_renderer(render_files_content);

        wm.add_window(files_win);
        wm.set_active(WindowId::new(4));
        log::info!("[Files] Spawned new Files window (ID: 4) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global FileManagerApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    FILE_MANAGER.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global FileManagerApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    FILE_MANAGER.lock().handle_key(key)
}
