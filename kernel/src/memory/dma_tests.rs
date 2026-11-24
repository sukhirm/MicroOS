//! Unit tests for Bounded DMA Memory Allocation Primitives.

use x86_64::{PhysAddr, VirtAddr};
use crate::memory::dma::{
    active_dma_bytes, reset_dma_accounting, DmaBuffer, DmaError, MAX_DMA_POOL_BYTES,
};

/// Runs all in-kernel DMA unit tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("         Running Micro-OS Phase 5 Bounded DMA Tests             ");
    log::info!("================================================================");

    test_dma_zero_size_rejection();
    test_dma_invalid_alignment_rejection();
    test_dma_capacity_boundary();
    test_dma_buffer_bounds_checking();
    test_dma_data_integrity_and_zeroing();
    test_dma_memory_barrier();
    test_dma_accounting_metrics();

    log::info!("================================================================");
    log::info!("         [OK] All Bounded DMA Memory Tests Passed!              ");
    log::info!("================================================================");
}

/// 1. Verifies that requesting a DMA buffer of size 0 returns DmaError::ZeroSize.
fn test_dma_zero_size_rejection() {
    let res = DmaBuffer::allocate(0, 64);
    assert_eq!(res.unwrap_err(), DmaError::ZeroSize);
    log::info!("  [PASS] 1. Zero-size DMA allocation rejection verified");
}

/// 2. Verifies that non-power-of-two alignments are rejected.
fn test_dma_invalid_alignment_rejection() {
    assert_eq!(DmaBuffer::allocate(4096, 0).unwrap_err(), DmaError::InvalidAlignment);
    assert_eq!(DmaBuffer::allocate(4096, 3).unwrap_err(), DmaError::InvalidAlignment);
    assert_eq!(DmaBuffer::allocate(4096, 15).unwrap_err(), DmaError::InvalidAlignment);
    assert_eq!(DmaBuffer::allocate(4096, 127).unwrap_err(), DmaError::InvalidAlignment);
    log::info!("  [PASS] 2. Non-power-of-two DMA alignment rejection verified");
}

/// 3. Verifies that requesting more than the maximum 4 MiB pool is rejected.
fn test_dma_capacity_boundary() {
    let res = DmaBuffer::allocate(MAX_DMA_POOL_BYTES + 4096, 4096);
    assert_eq!(res.unwrap_err(), DmaError::CapacityExceeded);
    log::info!("  [PASS] 3. Maximum DMA pool boundary (4 MiB) enforcement verified");
}

/// 4. Verifies bounds checking on read_bytes and write_bytes.
fn test_dma_buffer_bounds_checking() {
    let mut backing = [0u8; 128];
    let virt = VirtAddr::new(backing.as_mut_ptr() as u64);
    let mut buf = DmaBuffer::from_raw(virt, PhysAddr::new(0x10000), 128, 64);
    assert!(!buf.owns_pool_allocation(), "from_raw buffer must not own pool allocation");

    let mut read_buf = [0u8; 64];
    // In-bounds read
    assert!(buf.read_bytes(0, &mut read_buf).is_ok());
    assert!(buf.read_bytes(64, &mut read_buf).is_ok());

    // Out-of-bounds read
    assert_eq!(buf.read_bytes(65, &mut read_buf).unwrap_err(), DmaError::OutOfBounds);
    assert_eq!(buf.read_bytes(128, &mut read_buf).unwrap_err(), DmaError::OutOfBounds);

    // In-bounds write
    let write_data = [0xFFu8; 32];
    assert!(buf.write_bytes(0, &write_data).is_ok());
    assert!(buf.write_bytes(96, &write_data).is_ok());

    // Out-of-bounds write
    assert_eq!(buf.write_bytes(97, &write_data).unwrap_err(), DmaError::OutOfBounds);

    log::info!("  [PASS] 4. DMA buffer offset and length bounds checking verified");
}

/// 5. Verifies data integrity, slice access, and zero-initialization.
fn test_dma_data_integrity_and_zeroing() {
    let mut backing = [0xAAu8; 64];
    let virt = VirtAddr::new(backing.as_mut_ptr() as u64);
    let mut buf = DmaBuffer::from_raw(virt, PhysAddr::new(0x20000), 64, 64);
    assert!(!buf.owns_pool_allocation(), "from_raw buffer must not own pool allocation");

    buf.zero();
    for &b in buf.as_slice() {
        assert_eq!(b, 0, "Buffer must be zeroed");
    }

    let payload = [1, 2, 3, 4, 5, 6, 7, 8];
    buf.write_bytes(16, &payload).unwrap();

    let mut read_back = [0u8; 8];
    buf.read_bytes(16, &mut read_back).unwrap();
    assert_eq!(read_back, payload);

    log::info!("  [PASS] 5. DMA buffer data integrity and zero-initialization verified");
}

/// 6. Verifies that the hardware memory barrier compiles and executes cleanly.
fn test_dma_memory_barrier() {
    DmaBuffer::memory_barrier();
    log::info!("  [PASS] 6. DMA hardware memory barrier execution verified");
}

/// 7. Verifies active DMA memory allocation accounting, lifecycle, and reset.
fn test_dma_accounting_metrics() {
    let before = active_dma_bytes();
    assert_eq!(
        before, 0,
        "Initial active DMA bytes must be zero (unowned buffer drops must not underflow)"
    );

    // Verify dynamic allocation and drop lifecycle if frame allocator is active
    if let Ok(buf) = DmaBuffer::allocate(4096, 4096) {
        assert_eq!(buf.size(), 4096);
        assert_eq!(buf.alignment(), 4096);
        assert!(buf.owns_pool_allocation());
        assert_eq!(
            active_dma_bytes(),
            4096,
            "Active DMA pool bytes must reflect 4 KiB allocation"
        );
        drop(buf);
        assert_eq!(
            active_dma_bytes(),
            0,
            "Active DMA pool bytes must return to 0 after drop"
        );
    }

    reset_dma_accounting();
    assert_eq!(active_dma_bytes(), 0);
    log::info!("  [PASS] 7. DMA active memory accounting, lifecycle, and reset verified");
}
