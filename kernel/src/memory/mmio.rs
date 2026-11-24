//! Device Memory-Mapped I/O (MMIO) Mapping Subsystem.
//!
//! Provides bounded, verified virtual address space mapping for physical PCI/device
//! registers with proper page-table caching flags (`PageTableFlags::NO_CACHE | WRITABLE`)
//! and strict alignment and bounds checking.

#![allow(dead_code)]

use spin::Mutex;
use x86_64::structures::paging::{
    Mapper, Page, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

use crate::memory::paging::init_offset_page_table;
use crate::memory::{phys_mem_offset, FRAME_ALLOCATOR};

/// Base virtual address reserved for device MMIO mappings.
/// Resides in canonical 48-bit address space, distinct from heap (`0x4444_4444_0000`)
/// and backbuffer (`0x4444_5000_0000`).
pub const MMIO_VIRT_START: u64 = 0x0000_4444_6000_0000;

/// Maximum virtual space reserved for device MMIO (64 MiB).
pub const MMIO_MAX_RESERVED_BYTES: usize = 64 * 1024 * 1024;

/// 4 KiB standard page size.
pub const PAGE_SIZE: usize = 4096;

/// Global spinlock-protected MMIO allocation tracker.
pub static MMIO_MANAGER: Mutex<MmioManager> = Mutex::new(MmioManager::new());

/// Errors that can occur during MMIO mapping or access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioError {
    ZeroSize,
    UnalignedPhysicalAddress,
    CapacityExceeded,
    MappingFailed,
    OutOfBoundsAccess,
    PhysicalMemoryOffsetNotSet,
    FrameAllocatorUnavailable,
}

/// A mapped, bounded hardware MMIO region.
#[derive(Debug, Clone, Copy)]
pub struct MmioRegion {
    phys_addr: PhysAddr,
    virt_addr: VirtAddr,
    size: usize,
}

impl MmioRegion {
    pub const fn new(phys_addr: PhysAddr, virt_addr: VirtAddr, size: usize) -> Self {
        Self {
            phys_addr,
            virt_addr,
            size,
        }
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.phys_addr
    }

    pub fn virt_addr(&self) -> VirtAddr {
        self.virt_addr
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Reads an 8-bit register at the specified byte offset.
    pub unsafe fn read_u8(&self, offset: usize) -> Result<u8, MmioError> {
        if offset + 1 > self.size {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *const u8;
        Ok(core::ptr::read_volatile(ptr))
    }

    /// Reads a 16-bit register at the specified byte offset.
    pub unsafe fn read_u16(&self, offset: usize) -> Result<u16, MmioError> {
        if offset + 2 > self.size || offset % 2 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *const u16;
        Ok(core::ptr::read_volatile(ptr))
    }

    /// Reads a 32-bit register at the specified byte offset.
    pub unsafe fn read_u32(&self, offset: usize) -> Result<u32, MmioError> {
        if offset + 4 > self.size || offset % 4 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *const u32;
        Ok(core::ptr::read_volatile(ptr))
    }

    /// Reads a 64-bit register at the specified byte offset.
    pub unsafe fn read_u64(&self, offset: usize) -> Result<u64, MmioError> {
        if offset + 8 > self.size || offset % 8 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *const u64;
        Ok(core::ptr::read_volatile(ptr))
    }

    /// Writes an 8-bit register at the specified byte offset.
    pub unsafe fn write_u8(&self, offset: usize, value: u8) -> Result<(), MmioError> {
        if offset + 1 > self.size {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *mut u8;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }

    /// Writes a 16-bit register at the specified byte offset.
    pub unsafe fn write_u16(&self, offset: usize, value: u16) -> Result<(), MmioError> {
        if offset + 2 > self.size || offset % 2 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *mut u16;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }

    /// Writes a 32-bit register at the specified byte offset.
    pub unsafe fn write_u32(&self, offset: usize, value: u32) -> Result<(), MmioError> {
        if offset + 4 > self.size || offset % 4 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *mut u32;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }

    /// Writes a 64-bit register at the specified byte offset.
    pub unsafe fn write_u64(&self, offset: usize, value: u64) -> Result<(), MmioError> {
        if offset + 8 > self.size || offset % 8 != 0 {
            return Err(MmioError::OutOfBoundsAccess);
        }
        let ptr = (self.virt_addr.as_u64() + offset as u64) as *mut u64;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }
}

/// Tracks virtual address allocations within the reserved MMIO window.
pub struct MmioManager {
    allocated_bytes: usize,
}

impl MmioManager {
    pub const fn new() -> Self {
        Self { allocated_bytes: 0 }
    }

    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }

    /// Maps a physical MMIO memory range into the reserved virtual space with `NO_CACHE` flags.
    pub fn map_range(&mut self, phys_addr: PhysAddr, size: usize) -> Result<MmioRegion, MmioError> {
        if size == 0 {
            return Err(MmioError::ZeroSize);
        }

        // Physical address must be 4 KiB page-aligned
        if phys_addr.as_u64() % PAGE_SIZE as u64 != 0 {
            return Err(MmioError::UnalignedPhysicalAddress);
        }

        // Align size up to next 4 KiB boundary
        let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        if self.allocated_bytes + aligned_size > MMIO_MAX_RESERVED_BYTES {
            return Err(MmioError::CapacityExceeded);
        }

        let virt_start = VirtAddr::new(MMIO_VIRT_START + self.allocated_bytes as u64);
        let phys_offset = phys_mem_offset();
        if phys_offset == 0 {
            return Err(MmioError::PhysicalMemoryOffsetNotSet);
        }

        let mut frame_alloc_guard = FRAME_ALLOCATOR.lock();
        let frame_allocator = frame_alloc_guard
            .as_mut()
            .ok_or(MmioError::FrameAllocatorUnavailable)?;

        // Map pages into active page table
        let mut mapper = unsafe { init_offset_page_table(VirtAddr::new(phys_offset)) };
        let page_count = aligned_size / PAGE_SIZE;

        // Flags: PRESENT | WRITABLE | NO_CACHE | NO_EXECUTE
        let flags = PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_CACHE
            | PageTableFlags::NO_EXECUTE;

        for i in 0..page_count {
            let page_virt = virt_start + (i * PAGE_SIZE) as u64;
            let page = Page::<Size4KiB>::containing_address(page_virt);
            let frame_phys = phys_addr + (i * PAGE_SIZE) as u64;
            let frame = PhysFrame::<Size4KiB>::containing_address(frame_phys);

            // If page is already mapped, fail cleanly
            if mapper.translate_page(page).is_ok() {
                return Err(MmioError::MappingFailed);
            }

            unsafe {
                if mapper.map_to(page, frame, flags, frame_allocator).is_err() {
                    return Err(MmioError::MappingFailed);
                }
                // Flush TLB for mapped page
                x86_64::instructions::tlb::flush(page_virt);
            }
        }

        self.allocated_bytes += aligned_size;
        Ok(MmioRegion::new(phys_addr, virt_start, aligned_size))
    }
}

/// Convenience function to map a device MMIO range using the global MMIO manager.
pub fn map_device_mmio(phys_addr: PhysAddr, size: usize) -> Result<MmioRegion, MmioError> {
    MMIO_MANAGER.lock().map_range(phys_addr, size)
}
