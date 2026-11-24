//! Memory Management Subsystem for Micro-OS.
//!
//! Provides physical frame allocation, virtual memory page table mapping,
//! a 16 MiB kernel heap, thread-safe linked-list allocation, and comprehensive self-tests.

pub mod allocator;
pub mod dma;
pub mod dma_tests;
pub mod frame;
pub mod mmio;
pub mod mmio_tests;
pub mod paging;

#[allow(unused_imports)]
pub use allocator::{
    allocation_stats, atomic_saturating_inc, benchmark_allocator_latency, heap_diagnostics,
    heap_fragmentation, reset_allocation_stats, AllocatorBenchmarkReport, AllocatorStats,
    HeapDiagnostics, HeapFragmentation, SizeLatencyMeasurement, BENCHMARK_REPETITIONS,
    BENCHMARK_SIZES,
};

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use core::alloc::Layout;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use x86_64::VirtAddr;

/// Total usable conventional physical memory in bytes detected by the UEFI bootloader.
pub static TOTAL_USABLE_RAM_BYTES: AtomicU64 = AtomicU64::new(0);

/// Total memory map regions reported by UEFI.
pub static TOTAL_REGIONS_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Total usable memory regions reported by UEFI.
pub static USABLE_REGIONS_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Total 4 KiB physical frames allocated by the frame allocator.
pub static ALLOCATED_FRAMES_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Global physical memory offset provided by the UEFI bootloader for dynamic direct-mapped RAM.
pub static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Global spinlock-protected physical frame allocator for runtime kernel allocations (MMIO tables & DMA).
pub static FRAME_ALLOCATOR: spin::Mutex<Option<frame::BootInfoFrameAllocator>> = spin::Mutex::new(None);

/// Returns the dynamic physical memory offset.
pub fn phys_mem_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Relaxed)
}

/// Returns the total usable physical RAM in bytes.
pub fn total_usable_ram_bytes() -> u64 {
    let bytes = TOTAL_USABLE_RAM_BYTES.load(Ordering::Relaxed);
    if bytes > 0 {
        bytes
    } else {
        // Fallback default (497 MiB) if accessed prior to bootloader initialization or in unit tests
        497 * 1024 * 1024
    }
}

/// Returns the total number of physical 4 KiB frames allocated for kernel structures.
pub fn allocated_frames_count() -> usize {
    ALLOCATED_FRAMES_COUNT.load(Ordering::Relaxed)
}

/// Returns the current number of bytes dynamically allocated from the kernel heap.
pub fn heap_allocated_bytes() -> usize {
    allocator::ALLOCATOR.lock().allocated_bytes()
}

/// Returns the total kernel heap capacity in bytes (16 MiB = 16,777,216 bytes).
pub fn heap_total_bytes() -> usize {
    HEAP_SIZE
}

/// Fixed virtual address where the kernel heap begins (48-bit canonical virtual address).
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// Fixed size of the kernel heap: 16 MiB (4096 pages of 4 KiB each).
pub const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Fixed virtual address where the dedicated 16 MiB graphics backbuffer begins.
pub const BACKBUFFER_START: usize = 0x_4444_5000_0000;

/// Fixed size of the graphics backbuffer: 16 MiB (4096 pages of 4 KiB each).
/// Supports up to 2560x1440 QHD (14.7 MB) and 1920x1080 FHD (8.3 MB) 32-bit framebuffers.
pub const BACKBUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Initializes the complete memory management subsystem:
/// 1. Physical frame allocator backed by bootloader memory map.
/// 2. Active Level-4 page table mapper.
/// 3. Virtual memory mapping for the 16 MiB kernel heap.
/// 4. Linked-list heap allocator initialized and registered for `alloc`.
pub fn init(memory_regions: &'static MemoryRegions, phys_mem_offset: u64) {
    let phys_mem_virt = VirtAddr::new(phys_mem_offset);

    // Compute and record physical memory metrics from bootloader memory regions
    let mut usable_bytes: u64 = 0;
    let mut usable_regions: usize = 0;
    for region in memory_regions.iter() {
        if region.kind == MemoryRegionKind::Usable {
            usable_regions += 1;
            usable_bytes += region.end - region.start;
        }
    }
    TOTAL_USABLE_RAM_BYTES.store(usable_bytes, Ordering::Relaxed);
    TOTAL_REGIONS_COUNT.store(memory_regions.len(), Ordering::Relaxed);
    USABLE_REGIONS_COUNT.store(usable_regions, Ordering::Relaxed);
    PHYS_MEM_OFFSET.store(phys_mem_offset, Ordering::SeqCst);

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

    let backbuffer_start = VirtAddr::new(BACKBUFFER_START as u64);
    log::info!(
        "Mapping dedicated graphics backbuffer: {:#018x} - {:#018x} ({} KiB)...",
        BACKBUFFER_START,
        BACKBUFFER_START + BACKBUFFER_SIZE,
        BACKBUFFER_SIZE / 1024
    );
    paging::map_memory_region(backbuffer_start, BACKBUFFER_SIZE, &mut mapper, &mut frame_allocator)
        .expect("Failed to map graphics backbuffer virtual memory");
    unsafe {
        core::ptr::write_bytes(BACKBUFFER_START as *mut u8, 0, BACKBUFFER_SIZE);
    }
    log::info!(
        "Graphics backbuffer mapped successfully ({} total physical frames allocated).",
        frame_allocator.allocated_frames()
    );
    ALLOCATED_FRAMES_COUNT.store(frame_allocator.allocated_frames(), Ordering::Relaxed);

    log::info!("Initializing global linked-list heap allocator...");
    // SAFETY: Heap pages were mapped and verified above.
    unsafe {
        allocator::ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }
    log::info!("Kernel heap allocator registered and active.");

    // Store frame allocator for subsequent runtime driver allocations (MMIO intermediate tables & DMA)
    *FRAME_ALLOCATOR.lock() = Some(frame_allocator);
    log::info!("Global frame allocator registered for MMIO and DMA subsystem.");
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
        // Attempt an allocation larger than the entire 16 MiB heap
        let oversized_layout = Layout::from_size_align(HEAP_SIZE * 2, 16).unwrap();
        // SAFETY: We test the low-level GlobalAlloc directly to verify graceful null return without panic
        let ptr = unsafe {
            core::alloc::GlobalAlloc::alloc(&allocator::ALLOCATOR, oversized_layout)
        };
        assert!(ptr.is_null(), "Oversized allocation must return null pointer");
        log::info!("  [PASS] Allocation failure handling (request for 32 MiB cleanly returned null)");
    }

    // 7. Heap fragmentation telemetry verification
    {
        let frag = heap_fragmentation();
        assert!(frag.total_free_bytes > 0, "Heap free bytes must be > 0");
        assert!(frag.largest_free_block_bytes <= frag.total_free_bytes, "Largest block cannot exceed total free");
        assert!(frag.fragmentation_permille <= 1000, "Fragmentation permille cannot exceed 1000");
        assert!(frag.fragmentation_f32 >= 0.0 && frag.fragmentation_f32 <= 1.0, "Fragmentation f32 must be in [0.0, 1.0]");
        log::info!(
            "  [PASS] Heap fragmentation telemetry test (total free: {} B, largest: {} B, blocks: {}, frag: {}/1000, {:.2}%)",
            frag.total_free_bytes,
            frag.largest_free_block_bytes,
            frag.free_blocks_count,
            frag.fragmentation_permille,
            frag.fragmentation_f32 * 100.0
        );
    }

    // 8. Allocation statistics & counter boundaries verification
    {
        let stats = allocation_stats();
        assert!(stats.success_count > 0, "Success count must be > 0 after prior test allocations");
        assert!(stats.fail_count >= 1, "Fail count must be >= 1 after oversized allocation test");
        assert!(stats.dealloc_count > 0, "Dealloc count must be > 0 after prior drops");
        log::info!(
            "  [PASS] Allocator telemetry counters test (success: {}, fail: {}, dealloc: {}, live: {} B)",
            stats.success_count,
            stats.fail_count,
            stats.dealloc_count,
            stats.allocated_bytes
        );
    }

    // 9. On-demand allocator latency microbenchmark (16B, 64B, 256B, 1KiB, 4KiB)
    {
        let bytes_before = heap_allocated_bytes();
        let report = benchmark_allocator_latency();
        let bytes_after = heap_allocated_bytes();
        assert_eq!(
            bytes_before, bytes_after,
            "Heap allocated bytes must return to baseline after microbenchmark: before={}, after={}",
            bytes_before, bytes_after
        );
        log::info!(
            "  [PASS] Benchmark fragmentation: before={:.2}% ({}‰), after={:.2}% ({}‰)",
            report.fragmentation_before.fragmentation_f32 * 100.0,
            report.fragmentation_before.fragmentation_permille,
            report.fragmentation_after.fragmentation_f32 * 100.0,
            report.fragmentation_after.fragmentation_permille
        );

        for m in &report.measurements {
            assert!(m.success, "Benchmark allocation size {} B failed", m.size_bytes);
            log::info!(
                "  [PASS] Benchmark size {:>4} B: alloc = {:>5} cycles, dealloc = {:>5} cycles ({} reps)",
                m.size_bytes,
                m.alloc_cycles_median,
                m.dealloc_cycles_median,
                m.repetitions
            );
        }
    }

    log::info!("================================================================");
    log::info!("        [OK] All Phase 3A Heap Self-Tests Passed!               ");
    log::info!("================================================================");

    // Phase 5 Hardware Driver Foundation Tests (MMIO & DMA Primitives)
    mmio_tests::run_self_tests();
    dma_tests::run_self_tests();
}
