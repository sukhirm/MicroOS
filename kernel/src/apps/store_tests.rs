//! In-Kernel Unit and Self-Tests for the Software Store Application ("Store").
//!
//! Verifies package metadata integrity, unique package IDs, search query filtering,
//! category filtering, built-in vs. repository package status distinction,
//! invalid metadata rejection, safe install/uninstall state transitions (preventing fake downloads),
//! dependency formatting, empty catalogue search handling, sidebar/tab mouse hit-testing,
//! WindowManager registration, and rendering clipping safety.

use super::store::{
    open_or_focus_store_window, render_store_content, InstallStatus, Package, PackageCategory,
    PackageError, PackageSource, StoreApp, StoreView, STATIC_CATALOGUE,
};
use bootloader_api::info::PixelFormat;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::ICON_STORE;
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;
use crate::window::window::Window;
use alloc::vec;

/// Runs all in-kernel Software Store self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("       Running Micro-OS Phase 4E Software Store Self-Tests      ");
    log::info!("================================================================");

    test_package_metadata_validation();
    test_unique_package_ids();
    test_search_query_filtering();
    test_category_filtering();
    test_builtin_vs_available_distinction();
    test_invalid_package_metadata_rejection();
    test_installation_safety_transition();
    test_uninstallation_builtin_protection();
    test_dependency_metadata_verification();
    test_empty_catalogue_handling();
    test_store_initial_state_and_tab_navigation();
    test_store_mouse_click_hit_testing();
    test_store_window_integration();
    test_store_rendering_clipping_safety();

    log::info!("================================================================");
    log::info!("       [OK] All Phase 4E Software Store Self-Tests Passed!      ");
    log::info!("================================================================");
}

/// 1. Verifies package metadata fields and formatted size calculations.
fn test_package_metadata_validation() {
    assert_eq!(STATIC_CATALOGUE.len(), 10, "Catalogue must contain exactly 10 initial packages");

    for pkg in STATIC_CATALOGUE.iter() {
        assert!(pkg.validate_metadata(), "Package {} must pass validation", pkg.id);
        assert!(!pkg.formatted_size().is_empty());
    }

    log::info!("  [PASS] 1. Package metadata validation & size formatting test");
}

/// 2. Verifies that all packages in the catalogue have unique IDs (no duplicates).
fn test_unique_package_ids() {
    for i in 0..STATIC_CATALOGUE.len() {
        for j in (i + 1)..STATIC_CATALOGUE.len() {
            assert_ne!(
                STATIC_CATALOGUE[i].id,
                STATIC_CATALOGUE[j].id,
                "Duplicate package ID detected: {}",
                STATIC_CATALOGUE[i].id
            );
        }
    }

    log::info!("  [PASS] 2. Unique package IDs enforcement test");
}

/// 3. Verifies search query filtering across name, ID, and description.
fn test_search_query_filtering() {
    let mut app = StoreApp::new();

    // 1. Search "term" -> matches Terminal
    app.search_query = "term".into();
    let res1 = app.get_filtered_packages();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].id, "org.microos.terminal");

    // 2. Search "edit" -> matches Text Editor
    app.search_query = "edit".into();
    let res2 = app.get_filtered_packages();
    assert_eq!(res2.len(), 1);
    assert_eq!(res2[0].id, "org.microos.editor");

    // 3. Search "vfs" (in description) -> matches File Manager and Text Editor
    app.search_query = "vfs".into();
    let res3 = app.get_filtered_packages();
    assert!(res3.iter().any(|p| p.id == "org.microos.files"));
    assert!(res3.iter().any(|p| p.id == "org.microos.editor"));

    log::info!("  [PASS] 3. Search query filtering test");
}

/// 4. Verifies category tab filtering (Featured, System, Utilities, Productivity, Installed).
fn test_category_filtering() {
    let mut app = StoreApp::new();

    // 1. Featured Category
    app.select_category(PackageCategory::Featured);
    let featured = app.get_filtered_packages();
    assert!(!featured.is_empty());
    for p in &featured {
        assert!(p.is_featured, "Package {} in Featured must be marked featured", p.id);
    }

    // 2. System Category
    app.select_category(PackageCategory::System);
    let system_pkgs = app.get_filtered_packages();
    for p in &system_pkgs {
        assert_eq!(p.category, PackageCategory::System);
    }

    // 3. Installed Category
    app.select_category(PackageCategory::Installed);
    let installed = app.get_filtered_packages();
    for p in &installed {
        let st = app.get_package_status(p);
        assert!(st == InstallStatus::Builtin || st == InstallStatus::Installed);
    }

    log::info!("  [PASS] 4. Category tab filtering test");
}

/// 5. Verifies distinction between Built-in kernel apps and external repository packages.
fn test_builtin_vs_available_distinction() {
    let app = StoreApp::new();

    let files_pkg = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.files").unwrap();
    assert_eq!(app.get_package_status(files_pkg), InstallStatus::Builtin);

    let editor_pkg = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.editor").unwrap();
    assert_eq!(app.get_package_status(editor_pkg), InstallStatus::Available);

    log::info!("  [PASS] 5. Built-in vs. available repository status distinction test");
}

/// 6. Verifies that corrupt or invalid package metadata is properly rejected.
fn test_invalid_package_metadata_rejection() {
    let invalid_pkg1 = Package {
        id: "", // Empty ID
        name: "Test",
        version: "1.0",
        description: "Test",
        category: PackageCategory::Utilities,
        size_bytes: 100,
        dependencies: &[],
        status: InstallStatus::Available,
        source: PackageSource::Community,
        icon: &ICON_STORE,
        is_featured: false,
    };
    assert!(!invalid_pkg1.validate_metadata(), "Empty ID must fail validation");

    let invalid_pkg2 = Package {
        id: "no_dot_id", // Missing domain dot
        name: "Test",
        version: "1.0",
        description: "Test",
        category: PackageCategory::Utilities,
        size_bytes: 100,
        dependencies: &[],
        status: InstallStatus::Available,
        source: PackageSource::Community,
        icon: &ICON_STORE,
        is_featured: false,
    };
    assert!(!invalid_pkg2.validate_metadata(), "ID without domain dot must fail validation");

    let invalid_pkg3 = Package {
        id: "org.test",
        name: "", // Empty name
        version: "1.0",
        description: "Test",
        category: PackageCategory::Utilities,
        size_bytes: 100,
        dependencies: &[],
        status: InstallStatus::Available,
        source: PackageSource::Community,
        icon: &ICON_STORE,
        is_featured: false,
    };
    assert!(!invalid_pkg3.validate_metadata(), "Empty name must fail validation");

    log::info!("  [PASS] 6. Invalid package metadata rejection test");
}

/// 7. Verifies installation safety (never fakes installation when repository is offline).
fn test_installation_safety_transition() {
    let mut app = StoreApp::new();

    // Attempt to install external package "org.microos.editor"
    let result = app.install_package("org.microos.editor");
    assert_eq!(result, Err(PackageError::RepositoryOffline), "Must report repository offline");

    // Status must transition to Unavailable, NOT fake Installed
    let editor = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.editor").unwrap();
    assert_eq!(app.get_package_status(editor), InstallStatus::Unavailable);

    // Status banner must reflect reality
    let (msg, _) = app.status_message.expect("Must display status banner");
    assert!(msg.contains("Repository offline") || msg.contains("driver pending"));

    log::info!("  [PASS] 7. Installation safety transition & anti-fabrication test");
}

/// 8. Verifies that built-in system components are immutable and cannot be uninstalled.
fn test_uninstallation_builtin_protection() {
    let mut app = StoreApp::new();

    let result = app.uninstall_package("org.microos.files");
    assert_eq!(result, Err(PackageError::CannotUninstallBuiltin), "Built-in apps cannot be uninstalled");

    let files = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.files").unwrap();
    assert_eq!(app.get_package_status(files), InstallStatus::Builtin);

    let (msg, _) = app.status_message.expect("Must display status banner");
    assert!(msg.contains("immutable"));

    log::info!("  [PASS] 8. Uninstallation built-in component protection test");
}

/// 9. Verifies dependency metadata formatting.
fn test_dependency_metadata_verification() {
    let files = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.files").unwrap();
    assert_eq!(files.dependencies, &["core.vfs", "sys.gui"]);

    let terminal = STATIC_CATALOGUE.iter().find(|p| p.id == "org.microos.terminal").unwrap();
    assert_eq!(terminal.dependencies, &["core.vfs", "kernel.timer"]);

    log::info!("  [PASS] 9. Dependency metadata verification test");
}

/// 10. Verifies empty catalogue search behavior.
fn test_empty_catalogue_handling() {
    let mut app = StoreApp::new();
    app.search_query = "nonexistent_app_query_xyz".into();

    let filtered = app.get_filtered_packages();
    assert!(filtered.is_empty(), "Non-matching search must return empty list");

    log::info!("  [PASS] 10. Empty catalogue search handling test");
}

/// 11. Verifies Store initial state and category tab navigation.
fn test_store_initial_state_and_tab_navigation() {
    let mut app = StoreApp::new();

    assert_eq!(app.active_category, PackageCategory::All);
    assert_eq!(app.current_view, StoreView::Catalogue);
    assert!(app.search_query.is_empty());
    assert!(!app.search_focused);
    assert_eq!(app.scroll_offset, 0);

    // Tab key cycles category
    let tab_key = KeyEvent {
        code: KeyCode::Tab,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    assert!(app.handle_key(tab_key));
    assert_eq!(app.active_category, PackageCategory::Featured);

    log::info!("  [PASS] 11. Store initial state & tab navigation test");
}

/// 12. Verifies mouse click hit-testing for category tabs and package details.
fn test_store_mouse_click_hit_testing() {
    let mut app = StoreApp::new();
    let content_rect = Rect::new(100, 100, 580, 380);

    // 1. Click on search bar (content_rect.right() - 100, content_rect.y + 15)
    let clicked_search = app.handle_click(Point::new(content_rect.right() - 100, content_rect.y + 15), content_rect);
    assert!(clicked_search);
    assert!(app.search_focused);

    // 2. Click on Category Tab (Featured is 2nd tab, x ~ content_rect.x + 70, y = content_rect.y + 45)
    let clicked_tab = app.handle_click(Point::new(content_rect.x + 75, content_rect.y + 45), content_rect);
    assert!(clicked_tab);
    assert_eq!(app.active_category, PackageCategory::Featured);

    // 3. Click on first card body (x = content_rect.x + 100, y = content_rect.y + 90) -> opens Details
    let clicked_card = app.handle_click(Point::new(content_rect.x + 100, content_rect.y + 90), content_rect);
    assert!(clicked_card);
    assert_eq!(app.current_view, StoreView::Details);
    assert!(app.selected_package_id.is_some());

    // 4. Click Back button in Details view (x = content_rect.x + 50, y = content_rect.y + 20)
    let clicked_back = app.handle_click(Point::new(content_rect.x + 50, content_rect.y + 20), content_rect);
    assert!(clicked_back);
    assert_eq!(app.current_view, StoreView::Catalogue);

    log::info!("  [PASS] 12. Store mouse click hit-testing test");
}

/// 13. Verifies WindowManager integration (open or bring to front, focus, single-instance).
fn test_store_window_integration() {
    let mut wm = WindowManager::new(1280, 800);

    // 1. Initial open spawns Software Store window
    open_or_focus_store_window(&mut wm);
    assert_eq!(wm.windows().len(), 1);

    let win = &wm.windows()[0];
    assert_eq!(win.title(), "Software Store");
    assert_eq!(win.id().as_u32(), 7);
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(7));

    // 2. Open again: must NOT spawn duplicate window, must focus existing
    open_or_focus_store_window(&mut wm);
    assert_eq!(wm.windows().len(), 1, "Software Store must remain single-instance");
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(7));

    // 3. Minimize and re-open: must restore to Normal and bring to front
    wm.minimize_window(crate::window::state::WindowId::new(7));
    assert_eq!(
        wm.get_window(crate::window::state::WindowId::new(7)).unwrap().state(),
        WindowState::Minimized
    );

    open_or_focus_store_window(&mut wm);
    assert_eq!(
        wm.get_window(crate::window::state::WindowId::new(7)).unwrap().state(),
        WindowState::Normal
    );
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(7));

    log::info!("  [PASS] 13. Store window manager registration & focus integration test");
}

/// 14. Verifies rendering with surface boundary clipping does not overflow or panic.
fn test_store_rendering_clipping_safety() {
    let mut buffer = vec![0u8; 100 * 100 * 4];
    let mut surface = Surface::from_raw_parts(&mut buffer, 100, 100, 100, 4, PixelFormat::Bgr);

    let dummy_win = Window::new(
        crate::window::state::WindowId::new(7),
        "Software Store",
        10,
        10,
        80,
        80,
    );

    // Clipped small rect
    let content_rect = Rect::new(10, 10, 80, 80);
    render_store_content(&dummy_win, &mut surface, content_rect);

    // Degenerate zero-size rect
    let zero_rect = Rect::new(0, 0, 0, 0);
    render_store_content(&dummy_win, &mut surface, zero_rect);

    log::info!("  [PASS] 14. Store rendering clipping safety test");
}
