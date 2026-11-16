//! Virtual Memory Paging and Heap Mapping.
//!
//! Provides the Level-4 page table mapper via active CR3 translation and
//! maps the kernel heap into virtual memory with proper page alignment.

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

/// Initializes a new OffsetPageTable.
///
/// # Safety
/// The caller must guarantee that the complete physical memory is mapped to
/// virtual memory at the passed `physical_memory_offset`.
/// Also, this function must be only called once to avoid aliasing `&mut` references
/// to the active Level 4 page table.
pub unsafe fn init_offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    let level_4_table: &'static mut PageTable = &mut *page_table_ptr;
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// Errors that can occur during heap virtual memory mapping.
#[derive(Debug)]
#[allow(dead_code)]
pub enum HeapMapError {
    AddressAlreadyMapped(VirtAddr),
    PageTableMappingFailed(MapToError<Size4KiB>),
    FrameAllocationExhausted,
}

impl From<MapToError<Size4KiB>> for HeapMapError {
    fn from(err: MapToError<Size4KiB>) -> Self {
        HeapMapError::PageTableMappingFailed(err)
    }
}

/// Verifies and maps a virtual memory range with 4 KiB pages.
///
/// Ensures every page in `[start_addr, start_addr + size)` is currently unmapped
/// before allocating physical frames and creating Level 1..4 page table entries.
pub fn map_memory_region(
    start_addr: VirtAddr,
    size: usize,
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), HeapMapError> {
    let end_addr = start_addr + (size as u64) - 1u64;
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let end_page = Page::<Size4KiB>::containing_address(end_addr);
    let page_range = Page::range_inclusive(start_page, end_page);

    // 1. Verification pass: Ensure none of the proposed pages are already mapped
    for page in page_range {
        if mapper.translate_page(page).is_ok() {
            return Err(HeapMapError::AddressAlreadyMapped(page.start_address()));
        }
    }

    // 2. Mapping pass: Allocate physical frames and map each page with PRESENT | WRITABLE
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(HeapMapError::FrameAllocationExhausted)?;

        // SAFETY: The page was verified unmapped above, the allocated physical frame
        // comes from usable RAM via our verified frame allocator, and flags permit kernel RW.
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    Ok(())
}

/// Verifies and maps a virtual memory range for the kernel heap.
pub fn map_heap_pages(
    heap_start: VirtAddr,
    heap_size: usize,
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), HeapMapError> {
    map_memory_region(heap_start, heap_size, mapper, frame_allocator)
}
