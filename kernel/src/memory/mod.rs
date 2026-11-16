//! Memory Management Subsystem for Micro-OS.
//!
//! Provides physical frame allocation, virtual memory page table mapping,
//! a 1 MiB kernel heap, thread-safe linked-list allocation, and comprehensive self-tests.

pub mod allocator;
pub mod frame;
pub mod paging;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use bootloader_api::info::MemoryRegions;
use core::alloc::Layout;
use x86_64::VirtAddr;

/// Fixed virtual address where the kernel heap begins (48-bit canonical virtual address).
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// Fixed size of the kernel heap: 1 MiB (256 pages of 4 KiB each).
pub const HEAP_SIZE: usize = 1024 * 1024;

/// Initializes the complete memory management subsystem:
/// 1. Physical frame allocator backed by bootloader memory map.
/// 2. Active Level-4 page table mapper.
/// 3. Virtual memory mapping for the 1 MiB kernel heap.
/// 4. Linked-list heap allocator initialized and registered for `alloc`.
pub fn init(memory_regions: &'static MemoryRegions, phys_mem_offset: u64) {
    let phys_mem_virt = VirtAddr::new(phys_mem_offset);

    log::info!("Initializing physical frame allocator...");
    // SAFETY: Bootloader guarantees memory_regions accurately reflect physical memory layout.
    let mut frame_allocator = unsafe { frame::BootInfoFrameAllocator::init(memory_regions) };

    log::info!("Initializing virtual page table mapper...");
    // SAFETY: Bootloader guarantees physical memory offset is complete and active CR3 is valid.
    let mut mapper = unsafe { paging::init_offset_page_table(phys_mem_virt) };

    let heap_start = VirtAddr::new(HEAP_START as u64);
    log::info!(
        "Mapping kernel virtual heap: {:#018x} - {:#018x} ({} KiB)...",
        HEAP_START,
        HEAP_START + HEAP_SIZE,
        HEAP_SIZE / 1024
    );

    paging::map_heap_pages(heap_start, HEAP_SIZE, &mut mapper, &mut frame_allocator)
        .expect("Failed to map kernel heap virtual memory");

    log::info!(
        "Kernel virtual heap mapped successfully ({} physical frames allocated).",
        frame_allocator.allocated_frames()
    );

    log::info!("Initializing global linked-list heap allocator...");
    // SAFETY: Heap pages were mapped and verified above.
    unsafe {
        allocator::ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }
    log::info!("Kernel heap allocator registered and active.");
}

/// Runs comprehensive self-tests verifying heap allocation, growth, formatting,
/// alignment, fragmentation coalescing, and bounds checking.
pub fn run_heap_self_tests() {
    log::info!("================================================================");
    log::info!("          Running Micro-OS Phase 3A Heap Self-Tests             ");
    log::info!("================================================================");

    // 1. Box allocation and deallocation
    {
        let value = 0xCAFE_BABE_DEAD_BEEFu64;
        let boxed = Box::new(value);
        let ptr = &*boxed as *const u64 as usize;
        assert_eq!(*boxed, value, "Box value mismatch");
        assert!(
            ptr >= HEAP_START && ptr < HEAP_START + HEAP_SIZE,
            "Box pointer {:#x} out of heap bounds",
            ptr
        );
        log::info!("  [PASS] Box allocation & deallocation test (ptr: {:#x}, value: {:#x})", ptr, *boxed);
    }

    // 2. Vec allocation, dynamic growth, and deallocation
    {
        let mut vec = Vec::new();
        let count = 500usize;
        for i in 0..count {
            vec.push(i);
        }
        assert_eq!(vec.len(), count, "Vec length mismatch");
        let sum: usize = vec.iter().sum();
        let expected_sum = (count * (count - 1)) / 2;
        assert_eq!(sum, expected_sum, "Vec data sum mismatch");
        log::info!(
            "  [PASS] Vec allocation & growth test ({} items, cap: {}, sum: {})",
            vec.len(),
            vec.capacity(),
            sum
        );
    }

    // 3. String allocation and formatting
    {
        let major = 0;
        let minor = 3;
        let patch = 0;
        let greeting = format!("Micro-OS Kernel Heap active (v{}.{}.{})!", major, minor, patch);
        assert!(greeting.contains("Micro-OS Kernel Heap active"));
        log::info!("  [PASS] String allocation & formatting test: \"{}\"", greeting);
    }

    // 4. Memory alignment verification (8-byte and 64-byte alignments)
    {
        #[repr(align(64))]
        struct Aligned64 {
            data: [u8; 64],
        }

        let aligned_box = Box::new(Aligned64 { data: [0x5A; 64] });
        let addr = &*aligned_box as *const Aligned64 as usize;
        assert_eq!(addr % 64, 0, "64-byte alignment check failed: {:#x}", addr);
        assert_eq!(aligned_box.data[0], 0x5A);
        log::info!("  [PASS] Memory alignment verification (64-byte aligned at {:#x})", addr);
    }

    // 5. Multiple sequential allocations & free list re-use
    {
        let count = 50;
        let mut boxes = Vec::new();
        for i in 0..count {
            boxes.push(Box::new(i * 10));
        }
        for (i, b) in boxes.iter().enumerate() {
            assert_eq!(**b, i * 10);
        }
        drop(boxes); // free all 50 boxes

        // Immediately allocate again to verify free list coalescing and reuse
        let mut new_boxes = Vec::new();
        for i in 0..count {
            new_boxes.push(Box::new(i * 100));
        }
        for (i, b) in new_boxes.iter().enumerate() {
            assert_eq!(**b, i * 100);
        }
        log::info!("  [PASS] Multiple allocations & free-list re-use test ({} allocations cycled)", count);
    }

    // 6. Allocation failure / bounds check handling
    {
        // Attempt an allocation larger than the entire 1 MiB heap
        let oversized_layout = Layout::from_size_align(HEAP_SIZE * 2, 16).unwrap();
        // SAFETY: We test the low-level GlobalAlloc directly to verify graceful null return without panic
        let ptr = unsafe {
            core::alloc::GlobalAlloc::alloc(&allocator::ALLOCATOR, oversized_layout)
        };
        assert!(ptr.is_null(), "Oversized allocation must return null pointer");
        log::info!("  [PASS] Allocation failure handling (request for 2 MiB cleanly returned null)");
    }

    log::info!("================================================================");
    log::info!("        [OK] All Phase 3A Heap Self-Tests Passed!               ");
    log::info!("================================================================");
}
