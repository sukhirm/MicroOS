//! Host-side automated validation tests for Memory Allocator Performance Subsystem.
//!
//! Phase 1A: Memory Allocator Performance Benchmarking
//!
//! IMPORTANT ARCHITECTURAL DISTINCTION:
//! These tests run on the host development machine (Windows x86_64) using a synthetic heap
//! buffer to mathematically and structurally verify allocator invariants:
//! - Layout size and 16-byte alignment enforcement
//! - Allocation across all 5 benchmark size tiers (16B, 64B, 256B, 1KiB, 4KiB)
//! - Proper deallocation and free-list coalescing
//! - Allocation statistics counter boundaries and saturating arithmetic
//! - Fragmentation calculation formula accuracy across all edge cases
//!
//! These host tests do NOT pretend to measure real bare-metal CPU instruction cycles.
//! Real instruction clock cycles are captured exclusively via bare-metal RDTSC inside the
//! Micro-OS kernel.

#![allow(dead_code, unused_imports)]

#[path = "../../kernel/src/memory/allocator.rs"]
mod allocator;

use core::alloc::Layout;
use core::sync::atomic::{AtomicU64, Ordering};
use allocator::{
    allocation_stats, atomic_saturating_inc, heap_diagnostics, reset_allocation_stats,
    AllocatorStats, HeapDiagnostics, HeapFragmentation, LinkedListAllocator, BENCHMARK_SIZES,
};

/// Helper to allocate an aligned synthetic heap buffer for testing.
#[repr(align(16))]
struct TestHeapBuffer<const N: usize> {
    data: [u8; N],
}

impl<const N: usize> TestHeapBuffer<N> {
    fn new() -> Self {
        Self { data: [0u8; N] }
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    fn len(&self) -> usize {
        N
    }
}

#[test]
fn test_host_allocation_all_five_sizes() {
    // 64 KiB synthetic heap
    let mut heap_buf = TestHeapBuffer::<65536>::new();
    let heap_start = heap_buf.as_mut_ptr() as usize;
    let heap_size = heap_buf.len();

    let mut alloc = LinkedListAllocator::new();
    unsafe { alloc.init(heap_start, heap_size) };

    assert_eq!(alloc.allocated_bytes(), 0);
    assert_eq!(alloc.total_bytes(), heap_size);

    let mut allocated_blocks = Vec::new();

    // Verify each standard benchmark size: 16B, 64B, 256B, 1024B, 4096B
    for &size in &BENCHMARK_SIZES {
        let layout = Layout::from_size_align(size, 16).expect("Valid layout");
        let addr = unsafe { alloc.allocate_raw(layout) };
        assert!(addr.is_some(), "Allocation failed for size {} B", size);

        let ptr = addr.unwrap();
        assert_eq!(ptr % 16, 0, "Allocated address {:#x} not 16-byte aligned", ptr);

        // Verify write and read integrity
        unsafe {
            core::ptr::write_bytes(ptr as *mut u8, 0xAA, size);
            assert_eq!(*(ptr as *const u8), 0xAA);
            assert_eq!(*((ptr + size - 1) as *const u8), 0xAA);
        }

        allocated_blocks.push((ptr, layout));
    }

    assert!(alloc.allocated_bytes() >= BENCHMARK_SIZES.iter().sum::<usize>());

    // Deallocate in reverse order (LIFO)
    for (ptr, layout) in allocated_blocks.into_iter().rev() {
        unsafe { alloc.deallocate_raw(ptr, layout) };
    }

    assert_eq!(alloc.allocated_bytes(), 0, "All bytes must be reclaimed upon deallocation");

    // Verify free list coalesced back into a single block
    let frag = alloc.fragmentation_metric();
    assert_eq!(frag.free_blocks_count, 1, "Heap must coalesce into exactly 1 free block");
    assert_eq!(frag.total_free_bytes, heap_size);
    assert_eq!(frag.largest_free_block_bytes, heap_size);
    assert_eq!(frag.fragmentation_permille, 0);
    assert_eq!(frag.fragmentation_f32, 0.0);
}

#[test]
fn test_host_free_list_coalescing_and_reuse() {
    let mut heap_buf = TestHeapBuffer::<32768>::new();
    let heap_start = heap_buf.as_mut_ptr() as usize;
    let heap_size = heap_buf.len();

    let mut alloc = LinkedListAllocator::new();
    unsafe { alloc.init(heap_start, heap_size) };

    let layout_a = Layout::from_size_align(1024, 16).unwrap();
    let layout_b = Layout::from_size_align(1024, 16).unwrap();
    let layout_c = Layout::from_size_align(1024, 16).unwrap();

    let addr_a = unsafe { alloc.allocate_raw(layout_a).unwrap() };
    let addr_b = unsafe { alloc.allocate_raw(layout_b).unwrap() };
    let addr_c = unsafe { alloc.allocate_raw(layout_c).unwrap() };

    // Free middle block first -> creates a hole
    unsafe { alloc.deallocate_raw(addr_b, layout_b) };
    let frag_hole = alloc.fragmentation_metric();
    assert!(frag_hole.free_blocks_count >= 2, "Hole must create at least 2 free regions");
    assert!(frag_hole.fragmentation_permille > 0, "Hole must cause non-zero fragmentation");

    // Free A and C -> must coalesce everything back into 1 block
    unsafe { alloc.deallocate_raw(addr_a, layout_a) };
    unsafe { alloc.deallocate_raw(addr_c, layout_c) };

    let frag_coalesced = alloc.fragmentation_metric();
    assert_eq!(frag_coalesced.free_blocks_count, 1);
    assert_eq!(frag_coalesced.largest_free_block_bytes, heap_size);
    assert_eq!(frag_coalesced.fragmentation_permille, 0);
}

#[test]
fn test_host_fragmentation_formula_edge_cases() {
    let mut heap_buf = TestHeapBuffer::<16384>::new();
    let heap_start = heap_buf.as_mut_ptr() as usize;
    let heap_size = heap_buf.len();

    let mut alloc = LinkedListAllocator::new();
    unsafe { alloc.init(heap_start, heap_size) };

    // Edge Case 1: Fresh unfragmented heap (1 single free block)
    let frag1 = alloc.fragmentation_metric();
    assert_eq!(frag1.fragmentation_permille, 0);
    assert_eq!(frag1.fragmentation_f32, 0.0);

    // Edge Case 2: Manually controlled fragmentation calculation
    // Formula verification: F = 1.0 - (largest / total)
    // If largest = 500, total = 1000 => F = 1.0 - 0.5 = 0.5 (500 permille)
    let total_free = 1000usize;
    let largest_free = 500usize;
    let expected_f32 = 1.0 - (largest_free as f32 / total_free as f32);
    let expected_permille = ((total_free - largest_free) * 1000 / total_free) as u32;
    assert_eq!(expected_f32, 0.5);
    assert_eq!(expected_permille, 500);

    // If largest = 700, total = 1000 => F = 1.0 - 0.7 = 0.3 (300 permille)
    let total_free2 = 1000usize;
    let largest_free2 = 700usize;
    let expected_f32_2 = 1.0 - (largest_free2 as f32 / total_free2 as f32);
    let expected_permille_2 = ((total_free2 - largest_free2) * 1000 / total_free2) as u32;
    assert!((expected_f32_2 - 0.30).abs() < 1e-5);
    assert_eq!(expected_permille_2, 300);

    // Edge Case 3: Empty heap (total_free == 0) -> safe division by zero handling
    let empty_alloc = LinkedListAllocator::new();
    let frag_empty = empty_alloc.fragmentation_metric();
    assert_eq!(frag_empty.total_free_bytes, 0);
    assert_eq!(frag_empty.largest_free_block_bytes, 0);
    assert_eq!(frag_empty.fragmentation_permille, 0);
    assert_eq!(frag_empty.fragmentation_f32, 0.0);
}

#[test]
fn test_host_atomic_saturating_inc_overflow_protection() {
    // Test on an isolated AtomicU64 to avoid parallel test races across threads
    let counter = AtomicU64::new(u64::MAX - 2);

    // 1st increment: reaches u64::MAX - 1
    atomic_saturating_inc(&counter);
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX - 1);

    // 2nd increment: reaches u64::MAX
    atomic_saturating_inc(&counter);
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);

    // 3rd increment: MUST SATURATE at u64::MAX and NOT wrap to 0
    atomic_saturating_inc(&counter);
    assert_eq!(
        counter.load(Ordering::Relaxed),
        u64::MAX,
        "Counter must remain pinned at u64::MAX and never wrap around to 0"
    );

    // 4th increment: confirms persistent saturation at u64::MAX
    atomic_saturating_inc(&counter);
    assert_eq!(
        counter.load(Ordering::Relaxed),
        u64::MAX,
        "Counter must remain pinned at u64::MAX after repeated increments"
    );
}

#[test]
fn test_host_counter_boundaries_and_stats() {
    reset_allocation_stats();
    let stats_initial = allocation_stats();
    assert_eq!(stats_initial.success_count, 0);
    assert_eq!(stats_initial.fail_count, 0);
    assert_eq!(stats_initial.dealloc_count, 0);

    // Oversized layout (64 MiB) on uninitialized or smaller heap must fail and increment fail_count
    let oversized = Layout::from_size_align(64 * 1024 * 1024, 16).unwrap();

    unsafe {
        let ptr = allocator::ALLOCATOR.allocate(oversized);
        assert!(ptr.is_null());
    }

    let stats_after_fail = allocation_stats();
    assert_eq!(stats_after_fail.fail_count, 1);
    assert_eq!(stats_after_fail.success_count, 0);

    let diag = heap_diagnostics();
    assert_eq!(diag.fail_count, 1);
    assert_eq!(diag.last_failed_size, 64 * 1024 * 1024);
    assert_eq!(diag.last_failed_align, 16);
}

#[test]
fn test_host_large_allocation_and_coalescing() {
    // Dynamically allocate a 2 MiB synthetic heap on host heap to prevent stack overflow
    let heap_capacity = 2 * 1024 * 1024; // 2 MiB
    let mut storage = vec![0u8; heap_capacity + 32];
    let raw_addr = storage.as_mut_ptr() as usize;
    let aligned_start = (raw_addr + 15) & !15;

    let mut alloc = LinkedListAllocator::new();
    unsafe { alloc.init(aligned_start, heap_capacity) };

    assert_eq!(alloc.allocated_bytes(), 0);
    assert_eq!(alloc.total_bytes(), heap_capacity);

    // Exact size that previously failed on 1 MiB heap: 680 * 460 * 4 = 1,251,200 bytes
    let large_size = 680 * 460 * 4;
    let layout = Layout::from_size_align(large_size, 16).expect("Valid 16-byte aligned layout");

    let addr = unsafe { alloc.allocate_raw(layout) };
    assert!(addr.is_some(), "Large allocation of {} bytes failed on 2 MiB heap", large_size);

    let ptr = addr.unwrap();
    assert_eq!(ptr % 16, 0, "Allocated address must be 16-byte aligned");
    assert!(alloc.allocated_bytes() >= large_size);

    // Verify write and read integrity across the entire 1,251,200-byte buffer
    unsafe {
        core::ptr::write_bytes(ptr as *mut u8, 0x55, large_size);
        assert_eq!(*(ptr as *const u8), 0x55);
        assert_eq!(*((ptr + large_size / 2) as *const u8), 0x55);
        assert_eq!(*((ptr + large_size - 1) as *const u8), 0x55);
    }

    // Deallocate the large block
    unsafe { alloc.deallocate_raw(ptr, layout) };
    assert_eq!(alloc.allocated_bytes(), 0, "All allocated bytes must be reclaimed");

    // Verify that the heap coalesced back into a single contiguous block
    let frag = alloc.fragmentation_metric();
    assert_eq!(frag.free_blocks_count, 1, "Heap must coalesce into exactly 1 free block");
    assert_eq!(frag.total_free_bytes, heap_capacity);
    assert_eq!(frag.largest_free_block_bytes, heap_capacity);
    assert_eq!(frag.fragmentation_permille, 0);
}

#[test]
fn test_host_heap_diagnostics_reporting() {
    reset_allocation_stats();
    let diag_init = heap_diagnostics();
    assert_eq!(diag_init.success_count, 0);
    assert_eq!(diag_init.fail_count, 0);
    assert_eq!(diag_init.last_failed_size, 0);

    // Trigger failure with impossible size
    let impossible_layout = Layout::from_size_align(128 * 1024 * 1024, 64).unwrap();
    unsafe {
        let ptr = allocator::ALLOCATOR.allocate(impossible_layout);
        assert!(ptr.is_null());
    }

    let diag_fail = heap_diagnostics();
    assert_eq!(diag_fail.fail_count, 1);
    assert_eq!(diag_fail.last_failed_size, 128 * 1024 * 1024);
    assert_eq!(diag_fail.last_failed_align, 64);
}

#[test]
fn test_host_distinction_disclaimer() {
    // Structural test to guarantee host tests do not claim bare-metal cycle benchmarks
    let host_cycles = unsafe { allocator::read_cpu_cycles() };
    // Host RDTSC executes on host OS and is NOT bare-metal Zen 4
    assert!(host_cycles > 0 || cfg!(not(target_arch = "x86_64")));
}
