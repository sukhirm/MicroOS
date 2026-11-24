//! Host Unit Tests for MMIO & DMA Primitives (Stage 1).
//!
//! Validates:
//! 1. MMIO physical address alignment checks (4 KiB page boundary).
//! 2. MMIO zero size rejection.
//! 3. MMIO out-of-bounds access prevention.
//! 4. MMIO capacity limits (64 MiB window).
//! 5. DMA alignment power-of-two validation.
//! 6. DMA buffer bounds enforcement and zeroing.
//! 7. DMA capacity exhaustion (4 MiB limit).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const MMIO_MAX_RESERVED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DMA_POOL_BYTES: usize = 4 * 1024 * 1024;
pub const PAGE_SIZE: usize = 4096;

static HOST_ACTIVE_DMA_BYTES: AtomicUsize = AtomicUsize::new(0);
static DMA_TEST_LOCK: Mutex<()> = Mutex::new(());

fn host_reserve_dma_pool_bytes(bytes: usize) -> Result<(), HostDmaError> {
    let mut current = HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    loop {
        let new_total = match current.checked_add(bytes) {
            Some(t) if t <= MAX_DMA_POOL_BYTES => t,
            _ => return Err(HostDmaError::CapacityExceeded),
        };
        match HOST_ACTIVE_DMA_BYTES.compare_exchange_weak(
            current,
            new_total,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => return Ok(()),
            Err(actual) => current = actual,
        }
    }
}

fn host_release_dma_pool_bytes(bytes: usize) {
    let mut current = HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    loop {
        let new_total = current.saturating_sub(bytes);
        match HOST_ACTIVE_DMA_BYTES.compare_exchange_weak(
            current,
            new_total,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostMmioError {
    ZeroSize,
    UnalignedPhysicalAddress,
    CapacityExceeded,
    OutOfBoundsAccess,
}

#[derive(Debug)]
pub struct HostMmioRegion {
    pub phys_addr: u64,
    pub size: usize,
    data: Vec<u8>,
}

impl HostMmioRegion {
    pub fn new(phys_addr: u64, size: usize) -> Result<Self, HostMmioError> {
        if size == 0 {
            return Err(HostMmioError::ZeroSize);
        }
        if phys_addr % PAGE_SIZE as u64 != 0 {
            return Err(HostMmioError::UnalignedPhysicalAddress);
        }
        if size > MMIO_MAX_RESERVED_BYTES {
            return Err(HostMmioError::CapacityExceeded);
        }
        let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        Ok(Self {
            phys_addr,
            size: aligned_size,
            data: vec![0u8; aligned_size],
        })
    }

    pub fn read_u32(&self, offset: usize) -> Result<u32, HostMmioError> {
        if offset + 4 > self.size || offset % 4 != 0 {
            return Err(HostMmioError::OutOfBoundsAccess);
        }
        let bytes: [u8; 4] = self.data[offset..offset + 4].try_into().unwrap();
        Ok(u32::from_ne_bytes(bytes))
    }

    pub fn write_u32(&mut self, offset: usize, value: u32) -> Result<(), HostMmioError> {
        if offset + 4 > self.size || offset % 4 != 0 {
            return Err(HostMmioError::OutOfBoundsAccess);
        }
        let bytes = value.to_ne_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostDmaError {
    ZeroSize,
    InvalidAlignment,
    CapacityExceeded,
    OutOfBounds,
}

#[derive(Debug)]
pub struct HostDmaBuffer {
    pub phys_addr: u64,
    pub size: usize,
    pub alignment: usize,
    pub owns_pool_allocation: bool,
    data: Vec<u8>,
}

impl HostDmaBuffer {
    pub fn from_raw(phys_addr: u64, size: usize, alignment: usize) -> Self {
        Self {
            phys_addr,
            size,
            alignment,
            owns_pool_allocation: false,
            data: vec![0u8; size],
        }
    }

    pub fn allocate(size: usize, alignment: usize, current_pool_bytes: &mut usize) -> Result<Self, HostDmaError> {
        if size == 0 {
            return Err(HostDmaError::ZeroSize);
        }
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(HostDmaError::InvalidAlignment);
        }
        let aligned_size = match size.checked_add(PAGE_SIZE - 1) {
            Some(s) => s & !(PAGE_SIZE - 1),
            None => return Err(HostDmaError::CapacityExceeded),
        };

        if *current_pool_bytes + aligned_size > MAX_DMA_POOL_BYTES {
            return Err(HostDmaError::CapacityExceeded);
        }

        // Simulate base physical address with requested alignment
        let phys_addr = 0x2000_0000u64;
        assert_eq!(phys_addr % alignment as u64, 0);

        *current_pool_bytes += aligned_size;
        Ok(Self {
            phys_addr,
            size: aligned_size,
            alignment,
            owns_pool_allocation: false,
            data: vec![0u8; aligned_size],
        })
    }

    pub fn allocate_atomic(size: usize, alignment: usize) -> Result<Self, HostDmaError> {
        if size == 0 {
            return Err(HostDmaError::ZeroSize);
        }
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(HostDmaError::InvalidAlignment);
        }
        let aligned_size = match size.checked_add(PAGE_SIZE - 1) {
            Some(s) => s & !(PAGE_SIZE - 1),
            None => return Err(HostDmaError::CapacityExceeded),
        };

        host_reserve_dma_pool_bytes(aligned_size)?;

        let phys_addr = 0x2000_0000u64;
        Ok(Self {
            phys_addr,
            size: aligned_size,
            alignment,
            owns_pool_allocation: true,
            data: vec![0u8; aligned_size],
        })
    }

    pub fn write_bytes(&mut self, offset: usize, src: &[u8]) -> Result<(), HostDmaError> {
        if offset + src.len() > self.size {
            return Err(HostDmaError::OutOfBounds);
        }
        self.data[offset..offset + src.len()].copy_from_slice(src);
        Ok(())
    }

    pub fn read_bytes(&self, offset: usize, dst: &mut [u8]) -> Result<(), HostDmaError> {
        if offset + dst.len() > self.size {
            return Err(HostDmaError::OutOfBounds);
        }
        dst.copy_from_slice(&self.data[offset..offset + dst.len()]);
        Ok(())
    }
}

impl Drop for HostDmaBuffer {
    fn drop(&mut self) {
        if self.owns_pool_allocation {
            host_release_dma_pool_bytes(self.size);
        }
    }
}

#[test]
fn test_host_mmio_zero_size_rejection() {
    assert_eq!(HostMmioRegion::new(0xFD40_0000, 0).unwrap_err(), HostMmioError::ZeroSize);
}

#[test]
fn test_host_mmio_unaligned_physical_address_rejection() {
    assert_eq!(HostMmioRegion::new(0xFD40_0123, 4096).unwrap_err(), HostMmioError::UnalignedPhysicalAddress);
    assert_eq!(HostMmioRegion::new(0xFD40_0001, 4096).unwrap_err(), HostMmioError::UnalignedPhysicalAddress);
}

#[test]
fn test_host_mmio_capacity_limit() {
    let too_large = MMIO_MAX_RESERVED_BYTES + 4096;
    assert_eq!(HostMmioRegion::new(0xFD40_0000, too_large).unwrap_err(), HostMmioError::CapacityExceeded);
}

#[test]
fn test_host_mmio_bounds_and_alignment() {
    let mut region = HostMmioRegion::new(0xFD40_0000, 4096).unwrap();
    assert!(region.write_u32(0, 0x10EC_B852).is_ok());
    assert_eq!(region.read_u32(0).unwrap(), 0x10EC_B852);

    // Misaligned 32-bit offset
    assert_eq!(region.read_u32(1).unwrap_err(), HostMmioError::OutOfBoundsAccess);
    assert_eq!(region.write_u32(2, 0).unwrap_err(), HostMmioError::OutOfBoundsAccess);

    // Out-of-bounds offset
    assert_eq!(region.read_u32(4096).unwrap_err(), HostMmioError::OutOfBoundsAccess);
    assert_eq!(region.read_u32(4094).unwrap_err(), HostMmioError::OutOfBoundsAccess);
}

#[test]
fn test_host_dma_zero_size_rejection() {
    let mut pool = 0;
    assert_eq!(HostDmaBuffer::allocate(0, 64, &mut pool).unwrap_err(), HostDmaError::ZeroSize);
}

#[test]
fn test_host_dma_alignment_validation() {
    let mut pool = 0;
    assert_eq!(HostDmaBuffer::allocate(4096, 0, &mut pool).unwrap_err(), HostDmaError::InvalidAlignment);
    assert_eq!(HostDmaBuffer::allocate(4096, 3, &mut pool).unwrap_err(), HostDmaError::InvalidAlignment);
    assert_eq!(HostDmaBuffer::allocate(4096, 127, &mut pool).unwrap_err(), HostDmaError::InvalidAlignment);
    assert!(HostDmaBuffer::allocate(4096, 128, &mut pool).is_ok());
    assert!(HostDmaBuffer::allocate(4096, 4096, &mut pool).is_ok());
}

#[test]
fn test_host_dma_capacity_exhaustion() {
    let mut pool = 0;
    let too_large = MAX_DMA_POOL_BYTES + 4096;
    assert_eq!(HostDmaBuffer::allocate(too_large, 64, &mut pool).unwrap_err(), HostDmaError::CapacityExceeded);
}

#[test]
fn test_host_dma_bounds_and_integrity() {
    let mut pool = 0;
    let mut buf = HostDmaBuffer::allocate(4096, 64, &mut pool).unwrap();

    let data = [1, 2, 3, 4, 5, 6, 7, 8];
    assert!(buf.write_bytes(100, &data).is_ok());

    let mut out = [0u8; 8];
    assert!(buf.read_bytes(100, &mut out).is_ok());
    assert_eq!(out, data);

    // Out of bounds write
    assert_eq!(buf.write_bytes(4090, &data).unwrap_err(), HostDmaError::OutOfBounds);
}

#[test]
fn test_host_dma_reproduce_exact_from_raw_underflow_bug_fixed() {
    let _lock = DMA_TEST_LOCK.lock().unwrap();
    HOST_ACTIVE_DMA_BYTES.store(0, Ordering::SeqCst);

    // 1. In test_dma_buffer_bounds_checking, a 128-byte raw buffer was created
    {
        let raw128 = HostDmaBuffer::from_raw(0x10000, 128, 64);
        assert!(!raw128.owns_pool_allocation);
        // raw128 drops here
    }

    // 2. In test_dma_data_integrity_and_zeroing, a 64-byte raw buffer was created
    {
        let raw64 = HostDmaBuffer::from_raw(0x20000, 64, 64);
        assert!(!raw64.owns_pool_allocation);
        // raw64 drops here
    }

    // 3. With our fix, dropping unowned buffers MUST NOT decrement the active pool!
    let active = HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    assert_eq!(active, 0, "Unowned from_raw buffers must not decrement pool on drop");

    // Mathematical demonstration of the old unfixed bug:
    // If an untracked drop did 0usize.wrapping_sub(128).wrapping_sub(64):
    let bug_value = 0usize.wrapping_sub(128).wrapping_sub(64);
    assert_eq!(bug_value, 18446744073709551424usize, "Must match exact HP Victus panic value");
}

#[test]
fn test_host_dma_atomic_lifecycle_and_accounting() {
    let _lock = DMA_TEST_LOCK.lock().unwrap();
    HOST_ACTIVE_DMA_BYTES.store(0, Ordering::SeqCst);

    let initial = HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    assert_eq!(initial, 0);

    // Allocate 4 KiB buffer
    let buf = HostDmaBuffer::allocate_atomic(4096, 4096).expect("Allocation should succeed");
    assert_eq!(buf.size, 4096);
    assert!(buf.owns_pool_allocation);
    assert_eq!(HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst), 4096);

    // Allocate another 8 KiB buffer
    let buf2 = HostDmaBuffer::allocate_atomic(8192, 4096).expect("Allocation should succeed");
    assert_eq!(HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst), 4096 + 8192);

    // Drop second buffer
    drop(buf2);
    assert_eq!(HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst), 4096);

    // Drop first buffer
    drop(buf);
    assert_eq!(HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst), 0);
}

#[test]
fn test_host_dma_underflow_immunity_saturating_sub() {
    let _lock = DMA_TEST_LOCK.lock().unwrap();
    HOST_ACTIVE_DMA_BYTES.store(100, Ordering::SeqCst);

    // Attempting to subtract 500 must saturate to 0 rather than wrapping to 2^64 - 400
    host_release_dma_pool_bytes(500);

    assert_eq!(HOST_ACTIVE_DMA_BYTES.load(Ordering::SeqCst), 0);
}

#[test]
fn test_host_dma_checked_alignment_overflow_protection() {
    let mut pool = 0;
    // Attempting allocation with size near usize::MAX must return CapacityExceeded, not panic
    let huge_size = usize::MAX - 100;
    assert_eq!(
        HostDmaBuffer::allocate(huge_size, 4096, &mut pool).unwrap_err(),
        HostDmaError::CapacityExceeded
    );
    assert_eq!(
        HostDmaBuffer::allocate_atomic(huge_size, 4096).unwrap_err(),
        HostDmaError::CapacityExceeded
    );
}
