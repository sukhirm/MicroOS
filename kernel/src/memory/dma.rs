//! Direct Memory Access (DMA) Memory Management Primitives.
//!
//! Provides physically contiguous, bounded, and aligned memory buffers
//! with automatic physical-address tracking for PCIe and peripheral DMA controllers.

#![allow(dead_code)]

use core::sync::atomic::{AtomicUsize, Ordering};
use x86_64::structures::paging::FrameAllocator;
use x86_64::{PhysAddr, VirtAddr};

use crate::memory::{phys_mem_offset, FRAME_ALLOCATOR};

/// Maximum total memory pool reserved for DMA buffers (4 MiB).
pub const MAX_DMA_POOL_BYTES: usize = 4 * 1024 * 1024;

/// Standard page size (4 KiB).
pub const PAGE_SIZE: usize = 4096;

/// Total bytes currently allocated for active DMA buffers.
static ACTIVE_DMA_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Errors that can occur during DMA buffer allocation or manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    ZeroSize,
    InvalidAlignment,
    CapacityExceeded,
    FrameAllocationFailed,
    OutOfBounds,
    PhysicalMemoryOffsetNotSet,
}

/// Atomically reserves bytes in the active DMA pool with capacity enforcement.
fn reserve_dma_pool_bytes(bytes: usize) -> Result<(), DmaError> {
    let mut current = ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    loop {
        let new_total = match current.checked_add(bytes) {
            Some(t) if t <= MAX_DMA_POOL_BYTES => t,
            _ => return Err(DmaError::CapacityExceeded),
        };
        match ACTIVE_DMA_BYTES.compare_exchange_weak(
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

/// Atomically releases bytes from the active DMA pool with underflow protection.
fn release_dma_pool_bytes(bytes: usize) {
    let mut current = ACTIVE_DMA_BYTES.load(Ordering::SeqCst);
    loop {
        let new_total = current.saturating_sub(bytes);
        match ACTIVE_DMA_BYTES.compare_exchange_weak(
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

/// A physically contiguous, aligned DMA buffer with physical-to-virtual tracking.
#[derive(Debug)]
pub struct DmaBuffer {
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    size: usize,
    alignment: usize,
    owns_pool_allocation: bool,
}

impl DmaBuffer {
    /// Creates a DmaBuffer wrapping an existing mapped address (for test fixtures or fixed descriptors).
    ///
    /// This buffer does not own an allocation from the dynamic DMA pool and will not decrement
    /// active DMA memory accounting on drop.
    pub const fn from_raw(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        alignment: usize,
    ) -> Self {
        Self {
            virt_addr,
            phys_addr,
            size,
            alignment,
            owns_pool_allocation: false,
        }
    }

    /// Allocates a new zero-initialized DMA buffer of the specified size and alignment.
    ///
    /// Alignment must be a power of two and at least 64 bytes (cache line).
    pub fn allocate(size: usize, alignment: usize) -> Result<Self, DmaError> {
        if size == 0 {
            return Err(DmaError::ZeroSize);
        }

        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(DmaError::InvalidAlignment);
        }

        // Safe checked alignment calculation avoiding integer overflow
        let aligned_size = match size.checked_add(PAGE_SIZE - 1) {
            Some(s) => s & !(PAGE_SIZE - 1),
            None => return Err(DmaError::CapacityExceeded),
        };

        if aligned_size > MAX_DMA_POOL_BYTES {
            return Err(DmaError::CapacityExceeded);
        }

        // Atomically check and reserve capacity in the global DMA pool
        reserve_dma_pool_bytes(aligned_size)?;

        let phys_offset = phys_mem_offset();
        if phys_offset == 0 {
            release_dma_pool_bytes(aligned_size);
            return Err(DmaError::PhysicalMemoryOffsetNotSet);
        }

        let page_count = aligned_size / PAGE_SIZE;
        let mut frame_alloc_guard = FRAME_ALLOCATOR.lock();
        let frame_allocator = match frame_alloc_guard.as_mut() {
            Some(fa) => fa,
            None => {
                release_dma_pool_bytes(aligned_size);
                return Err(DmaError::FrameAllocationFailed);
            }
        };

        // Allocate first frame
        let first_frame = match frame_allocator.allocate_frame() {
            Some(f) => f,
            None => {
                release_dma_pool_bytes(aligned_size);
                return Err(DmaError::FrameAllocationFailed);
            }
        };
        let first_phys = first_frame.start_address();

        // Check alignment of base physical address
        if first_phys.as_u64() % alignment as u64 != 0 {
            release_dma_pool_bytes(aligned_size);
            return Err(DmaError::InvalidAlignment);
        }

        // Allocate remaining frames and guarantee physical contiguity
        for i in 1..page_count {
            let next_frame = match frame_allocator.allocate_frame() {
                Some(f) => f,
                None => {
                    release_dma_pool_bytes(aligned_size);
                    return Err(DmaError::FrameAllocationFailed);
                }
            };
            let expected_phys = first_phys + (i * PAGE_SIZE) as u64;
            if next_frame.start_address() != expected_phys {
                // Physical contiguity broken - roll back pool accounting
                release_dma_pool_bytes(aligned_size);
                return Err(DmaError::FrameAllocationFailed);
            }
        }

        // Explicitly release the spinlock before touching memory pages
        drop(frame_alloc_guard);

        // Calculate direct virtual address in physical map
        let virt_addr = VirtAddr::new(phys_offset + first_phys.as_u64());

        // Zero out the entire buffer
        unsafe {
            core::ptr::write_bytes(virt_addr.as_mut_ptr::<u8>(), 0, aligned_size);
        }

        Ok(Self {
            virt_addr,
            phys_addr: first_phys,
            size: aligned_size,
            alignment,
            owns_pool_allocation: true,
        })
    }

    pub fn virt_addr(&self) -> VirtAddr {
        self.virt_addr
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.phys_addr
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn alignment(&self) -> usize {
        self.alignment
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.virt_addr.as_ptr::<u8>(), self.size) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.virt_addr.as_mut_ptr::<u8>(), self.size) }
    }

    pub fn zero(&mut self) {
        unsafe {
            core::ptr::write_bytes(self.virt_addr.as_mut_ptr::<u8>(), 0, self.size);
        }
    }

    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), DmaError> {
        if offset + out.len() > self.size {
            return Err(DmaError::OutOfBounds);
        }
        unsafe {
            let src = (self.virt_addr.as_u64() + offset as u64) as *const u8;
            core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), out.len());
        }
        Ok(())
    }

    pub fn write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), DmaError> {
        if offset + data.len() > self.size {
            return Err(DmaError::OutOfBounds);
        }
        unsafe {
            let dst = (self.virt_addr.as_u64() + offset as u64) as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        }
        Ok(())
    }

    /// Whether this DMA buffer owns an active allocation from the global DMA pool.
    pub fn owns_pool_allocation(&self) -> bool {
        self.owns_pool_allocation
    }

    /// Full hardware memory barrier ensuring CPU writes are completed and
    /// visible to PCIe DMA controllers before triggering transaction.
    pub fn memory_barrier() {
        core::sync::atomic::fence(Ordering::SeqCst);
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        if self.owns_pool_allocation {
            release_dma_pool_bytes(self.size);
        }
    }
}

/// Returns the current number of bytes allocated in active DMA buffers.
pub fn active_dma_bytes() -> usize {
    ACTIVE_DMA_BYTES.load(Ordering::SeqCst)
}

/// Resets DMA accounting metrics (primarily for unit testing).
pub fn reset_dma_accounting() {
    ACTIVE_DMA_BYTES.store(0, Ordering::SeqCst);
}
