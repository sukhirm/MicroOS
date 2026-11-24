//! Unit tests for Hardware MMIO Mapping and Access Primitives.

use x86_64::{PhysAddr, VirtAddr};
use crate::memory::mmio::{
    MmioError, MmioManager, MmioRegion, MMIO_MAX_RESERVED_BYTES, PAGE_SIZE,
};

/// Runs all in-kernel MMIO unit tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("         Running Micro-OS Phase 5 Hardware MMIO Tests           ");
    log::info!("================================================================");

    test_mmio_zero_size_rejection();
    test_mmio_unaligned_address_rejection();
    test_mmio_bounds_enforcement();
    test_mmio_volatile_read_write();
    test_mmio_capacity_exhaustion();

    log::info!("================================================================");
    log::info!("         [OK] All Hardware MMIO Tests Passed!                   ");
    log::info!("================================================================");
}

/// 1. Verifies that requesting an MMIO mapping of size 0 returns MmioError::ZeroSize.
fn test_mmio_zero_size_rejection() {
    let mut mgr = MmioManager::new();
    let res = mgr.map_range(PhysAddr::new(0xFD40_0000), 0);
    assert_eq!(res.unwrap_err(), MmioError::ZeroSize);
    log::info!("  [PASS] 1. Zero size MMIO mapping rejection verified");
}

/// 2. Verifies that non-page-aligned physical addresses are rejected.
fn test_mmio_unaligned_address_rejection() {
    let mut mgr = MmioManager::new();
    let res = mgr.map_range(PhysAddr::new(0xFD40_0123), 4096);
    assert_eq!(res.unwrap_err(), MmioError::UnalignedPhysicalAddress);

    let res2 = mgr.map_range(PhysAddr::new(0xFD40_0FFF), 4096);
    assert_eq!(res2.unwrap_err(), MmioError::UnalignedPhysicalAddress);

    log::info!("  [PASS] 2. Unaligned MMIO physical address rejection verified");
}

/// 3. Verifies that out-of-bounds register accesses are prevented.
fn test_mmio_bounds_enforcement() {
    let mut backing = [0u8; 16];
    let virt = VirtAddr::new(backing.as_mut_ptr() as u64);
    let region = MmioRegion::new(PhysAddr::new(0x1000), virt, 16);

    unsafe {
        // In-bounds access
        assert!(region.read_u32(0).is_ok());
        assert!(region.read_u32(12).is_ok());
        assert!(region.write_u32(12, 0x12345678).is_ok());

        // Out-of-bounds offset
        assert_eq!(region.read_u32(13).unwrap_err(), MmioError::OutOfBoundsAccess);
        assert_eq!(region.read_u32(16).unwrap_err(), MmioError::OutOfBoundsAccess);
        assert_eq!(region.write_u32(14, 0x99).unwrap_err(), MmioError::OutOfBoundsAccess);

        // Misaligned access checks
        assert_eq!(region.read_u16(1).unwrap_err(), MmioError::OutOfBoundsAccess);
        assert_eq!(region.read_u32(2).unwrap_err(), MmioError::OutOfBoundsAccess);
        assert_eq!(region.read_u64(4).unwrap_err(), MmioError::OutOfBoundsAccess);
    }

    log::info!("  [PASS] 3. MMIO register bounds & alignment enforcement verified");
}

/// 4. Verifies volatile read/write semantics across multiple data widths.
fn test_mmio_volatile_read_write() {
    let mut backing = [0u8; 32];
    let virt = VirtAddr::new(backing.as_mut_ptr() as u64);
    let region = MmioRegion::new(PhysAddr::new(0x2000), virt, 32);

    unsafe {
        // 8-bit
        region.write_u8(0, 0x5A).unwrap();
        assert_eq!(region.read_u8(0).unwrap(), 0x5A);

        // 16-bit
        region.write_u16(2, 0xBEEF).unwrap();
        assert_eq!(region.read_u16(2).unwrap(), 0xBEEF);

        // 32-bit
        region.write_u32(4, 0xCAFEBABE).unwrap();
        assert_eq!(region.read_u32(4).unwrap(), 0xCAFEBABE);

        // 64-bit
        region.write_u64(8, 0x0123_4567_89AB_CDEF).unwrap();
        assert_eq!(region.read_u64(8).unwrap(), 0x0123_4567_89AB_CDEF);
    }

    log::info!("  [PASS] 4. Volatile MMIO read/write semantics (8, 16, 32, 64-bit) verified");
}

/// 5. Verifies that MMIO mapping beyond the maximum 64 MiB window is rejected.
fn test_mmio_capacity_exhaustion() {
    let mut mgr = MmioManager::new();
    let res = mgr.map_range(PhysAddr::new(0xFD40_0000), MMIO_MAX_RESERVED_BYTES + PAGE_SIZE);
    assert_eq!(res.unwrap_err(), MmioError::CapacityExceeded);

    log::info!("  [PASS] 5. MMIO window capacity boundary enforcement verified");
}
