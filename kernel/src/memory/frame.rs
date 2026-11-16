//! Physical Memory Frame Allocator.
//!
//! Provides a safe frame allocator backed by the bootloader-provided memory map.
//! Allocates 4 KiB physical frames from usable RAM regions while tracking
//! exhaustion and preventing allocation of occupied hardware frames.

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

/// A physical frame allocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    current_region_idx: usize,
    current_frame_offset: u64,
    allocated_frames: usize,
}

impl BootInfoFrameAllocator {
    /// Creates a FrameAllocator from the passed memory map.
    ///
    /// # Safety
    /// The caller must guarantee that the passed `memory_map` is valid and that
    /// all frames marked as `Usable` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        Self {
            memory_map,
            current_region_idx: 0,
            current_frame_offset: 0,
            allocated_frames: 0,
        }
    }

    /// Total number of physical 4 KiB frames allocated so far.
    pub fn allocated_frames(&self) -> usize {
        self.allocated_frames
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        while self.current_region_idx < self.memory_map.len() {
            let region = &self.memory_map[self.current_region_idx];

            // Only allocate from regions explicitly marked Usable by UEFI/bootloader
            if region.kind != MemoryRegionKind::Usable {
                self.current_region_idx += 1;
                self.current_frame_offset = 0;
                continue;
            }

            // Ensure 4 KiB alignment of the region start
            let aligned_start = (region.start + 0xfff) & !0xfff;
            let frame_addr = aligned_start + self.current_frame_offset;

            if frame_addr + 4096 <= region.end {
                self.current_frame_offset += 4096;
                self.allocated_frames += 1;
                return Some(PhysFrame::containing_address(PhysAddr::new(frame_addr)));
            } else {
                // Region exhausted, advance to next region
                self.current_region_idx += 1;
                self.current_frame_offset = 0;
            }
        }

        // All usable physical memory exhausted
        None
    }
}
