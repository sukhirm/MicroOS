//! Desktop Applications Subsystem for Micro-OS (Phase 4E).
//!
//! Hosts modular desktop graphical applications integrated with the Cosmic Slate desktop shell,
//! Window Manager, and input event dispatchers.

pub mod deadlock;
pub mod deadlock_tests;
pub mod files;
pub mod network;
pub mod network_tests;
pub mod settings;
pub mod settings_tests;
pub mod store;
pub mod store_tests;
pub mod sysmon;
pub mod sysmon_tests;
pub mod terminal;
pub mod terminal_tests;
pub mod tests;

pub fn run_self_tests() {
    tests::run_self_tests();
    settings_tests::run_self_tests();
    sysmon_tests::run_self_tests();
    network_tests::run_self_tests();
    terminal_tests::run_self_tests();
    store_tests::run_self_tests();
    deadlock_tests::run_self_tests();
}
