//! Kernel Heap Allocator & Performance Telemetry Subsystem.
//!
//! Provides a thread-safe linked-list allocator implementing `core::alloc::GlobalAlloc`.
//! Manages dynamic heap allocations, splits free blocks, coalesces adjacent regions,
//! tracks allocation/deallocation statistics, computes heap fragmentation metrics,
//! and executes on-demand latency microbenchmarks using bare-metal x86 RDTSC.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::mutex::SpinMutex;

/// Cumulative allocation telemetry counters.
static ALLOC_SUCCESS_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_FAIL_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Tracks the size and alignment of the most recent failed allocation request.
static LAST_FAILED_SIZE: AtomicUsize = AtomicUsize::new(0);
static LAST_FAILED_ALIGN: AtomicUsize = AtomicUsize::new(0);

/// Standard target allocation sizes for latency benchmarking (5 tiers: 16B, 64B, 256B, 1KiB, 4KiB).
pub const BENCHMARK_SIZES: [usize; 5] = [16, 64, 256, 1024, 4096];

/// Default repetitions per allocation size for median filtering.
pub const BENCHMARK_REPETITIONS: usize = 5;

/// Global allocation telemetry counters snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorStats {
    /// Total successful allocation operations completed.
    pub success_count: u64,
    /// Total failed allocation attempts (OOM / layout mismatch).
    pub fail_count: u64,
    /// Total deallocation operations completed.
    pub dealloc_count: u64,
    /// Total bytes dynamically allocated and currently live.
    pub allocated_bytes: usize,
    /// Total heap capacity in bytes.
    pub total_bytes: usize,
}

/// Comprehensive snapshot of heap metrics and runtime diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapDiagnostics {
    /// Total configured heap capacity in bytes.
    pub total_bytes: usize,
    /// Total bytes currently allocated by live heap objects.
    pub allocated_bytes: usize,
    /// Total free bytes available across the free list.
    pub total_free_bytes: usize,
    /// Size in bytes of the single largest contiguous free block.
    pub largest_free_block_bytes: usize,
    /// Total number of distinct free blocks in the free list.
    pub free_blocks_count: usize,
    /// Cumulative count of successful allocations.
    pub success_count: u64,
    /// Cumulative count of failed allocation requests.
    pub fail_count: u64,
    /// Cumulative count of deallocations.
    pub dealloc_count: u64,
    /// Size in bytes of the most recent failed allocation attempt (0 if none).
    pub last_failed_size: usize,
    /// Alignment in bytes of the most recent failed allocation attempt (0 if none).
    pub last_failed_align: usize,
}

/// Heap fragmentation telemetry metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeapFragmentation {
    /// Sum of all free block bytes currently in the free list.
    pub total_free_bytes: usize,
    /// Size in bytes of the single largest contiguous free block.
    pub largest_free_block_bytes: usize,
    /// Number of distinct non-contiguous free blocks in the free list.
    pub free_blocks_count: usize,
    /// Fragmentation index scaled to permille (parts per thousand: 0 to 1000).
    /// 0 = unfragmented (single free block or empty); 1000 = maximum fragmentation.
    pub fragmentation_permille: u32,
    /// Fragmentation index as f32 in range [0.0, 1.0].
    pub fragmentation_f32: f32,
}

/// Single latency benchmark measurement for a specific allocation size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeLatencyMeasurement {
    /// Target allocation size in bytes.
    pub size_bytes: usize,
    /// Median CPU clock cycles elapsed during allocation.
    pub alloc_cycles_median: u64,
    /// Median CPU clock cycles elapsed during deallocation.
    pub dealloc_cycles_median: u64,
    /// Number of timed repetitions executed.
    pub repetitions: usize,
    /// Whether all repetitions succeeded without null pointers.
    pub success: bool,
}

/// Comprehensive report returned by `benchmark_allocator_latency()`.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct AllocatorBenchmarkReport {
    /// Latency measurements across the 5 standard size tiers.
    pub measurements: [SizeLatencyMeasurement; 5],
    /// Heap fragmentation before the benchmark commenced.
    pub fragmentation_before: HeapFragmentation,
    /// Heap fragmentation after the benchmark concluded and all memory was freed.
    pub fragmentation_after: HeapFragmentation,
}

/// Reads the current CPU instruction clock cycle counter via x86 `rdtsc`.
///
/// Returns raw CPU instruction clock cycles. Does NOT convert to nanoseconds.
#[inline(always)]
pub unsafe fn read_cpu_cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_rdtsc()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}

/// A wrapper around `SpinMutex` to permit implementation of traits such as `GlobalAlloc`.
pub struct Locked<A> {
    inner: SpinMutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner: SpinMutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::mutex::SpinMutexGuard<'_, A> {
        self.inner.lock()
    }

    pub fn try_lock(&self) -> Option<spin::mutex::SpinMutexGuard<'_, A>> {
        self.inner.try_lock()
    }
}

impl Locked<LinkedListAllocator> {
    /// Allocates memory directly using the GlobalAlloc trait implementation.
    pub unsafe fn allocate(&self, layout: Layout) -> *mut u8 {
        core::alloc::GlobalAlloc::alloc(self, layout)
    }

    /// Deallocates memory directly using the GlobalAlloc trait implementation.
    pub unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        core::alloc::GlobalAlloc::dealloc(self, ptr, layout)
    }
}

/// A node in the free block linked list.
/// Stored directly in the unallocated memory of free blocks.
#[repr(C)]
struct ListNode {
    size: usize,
    next: *mut ListNode,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        Self {
            size,
            next: ptr::null_mut(),
        }
    }
}

/// A thread-safe linked-list allocator for the kernel heap.
pub struct LinkedListAllocator {
    head: ListNode,
    allocated_bytes: usize,
    total_bytes: usize,
}

impl LinkedListAllocator {
    /// Creates an empty LinkedListAllocator.
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
            allocated_bytes: 0,
            total_bytes: 0,
        }
    }

    /// Initializes the allocator with the given heap bounds.
    ///
    /// # Safety
    /// The caller must ensure that the memory region `[heap_start, heap_start + heap_size)`
    /// is mapped into virtual memory, writable, and not used elsewhere.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.total_bytes = heap_size;
        self.allocated_bytes = 0;
        self.head.next = ptr::null_mut();
        self.add_free_region(heap_start, heap_size);
    }

    /// Adds a freed memory region back into the address-sorted free list.
    pub unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // Enforce 16-byte alignment and minimum node capacity
        assert_eq!(addr % 16, 0, "Freed block address must be 16-byte aligned");
        assert!(size >= core::mem::size_of::<ListNode>(), "Freed block too small");

        let new_node = addr as *mut ListNode;
        new_node.write(ListNode::new(size));

        // Find the sorted insertion position
        let mut prev = &mut self.head as *mut ListNode;
        while !(*prev).next.is_null() && (*prev).next < new_node {
            prev = (*prev).next;
        }

        // Link new_node into the chain
        (*new_node).next = (*prev).next;
        (*prev).next = new_node;

        // Coalesce contiguous free blocks
        self.coalesce();
    }

    /// Merges contiguous free blocks in the sorted free list.
    pub unsafe fn coalesce(&mut self) {
        let mut check = &mut self.head as *mut ListNode;
        while !(*check).next.is_null() {
            let curr = (*check).next;
            let next = (*curr).next;
            if !next.is_null() && (curr as usize + (*curr).size == next as usize) {
                // Blocks are contiguous: absorb `next` into `curr`
                (*curr).size += (*next).size;
                (*curr).next = (*next).next;
            } else {
                check = curr;
            }
        }
    }

    /// Finds and extracts a suitable free block matching `size` and `align`.
    pub unsafe fn find_region(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev = &mut self.head as *mut ListNode;
        let mut curr = (*prev).next;

        while !curr.is_null() {
            let curr_start = curr as usize;
            let curr_end = curr_start + (*curr).size;
            let alloc_start = (curr_start + align - 1) & !(align - 1);
            let alloc_end = alloc_start + size;

            if alloc_end <= curr_end {
                let excess_before = alloc_start - curr_start;
                let excess_after = curr_end - alloc_end;
                let orig_next = (*curr).next;

                if excess_before >= 16 {
                    (*curr).size = excess_before;
                    if excess_after >= 16 {
                        let after_node = alloc_end as *mut ListNode;
                        after_node.write(ListNode {
                            size: excess_after,
                            next: orig_next,
                        });
                        (*curr).next = after_node;
                    }
                } else {
                    // No excess before: replace `curr` in the list
                    if excess_after >= 16 {
                        let after_node = alloc_end as *mut ListNode;
                        after_node.write(ListNode {
                            size: excess_after,
                            next: orig_next,
                        });
                        (*prev).next = after_node;
                    } else {
                        (*prev).next = orig_next;
                    }
                }

                self.allocated_bytes += size;
                return Some(alloc_start);
            }

            prev = curr;
            curr = (*curr).next;
        }

        None
    }

    /// Adjusts the layout size and alignment to ensure free list invariants (minimum 16 bytes).
    pub fn size_align(layout: Layout) -> (usize, usize) {
        let align = layout.align().max(16);
        let size = ((layout.size().max(16) + 15) / 16) * 16;
        (size, align)
    }

    /// Traverses the free list without performing ANY heap allocation.
    /// Returns (total_free_bytes, largest_free_block, free_block_count).
    ///
    /// # Safety
    /// Must be called with the allocator lock held or on an unshared instance.
    pub unsafe fn free_list_summary(&self) -> (usize, usize, usize) {
        let mut curr = self.head.next;
        let mut total_free = 0usize;
        let mut largest_free = 0usize;
        let mut count = 0usize;

        while !curr.is_null() {
            let size = (*curr).size;
            total_free = total_free.saturating_add(size);
            if size > largest_free {
                largest_free = size;
            }
            count = count.saturating_add(1);
            curr = (*curr).next;
        }

        (total_free, largest_free, count)
    }

    /// Calculates heap fragmentation metrics according to the standard formula:
    /// F = 1.0 - (largest_free_block / total_free_bytes)
    ///
    /// Safe against division by zero: if `total_free_bytes == 0` or `largest_free >= total_free`, F = 0.
    pub fn fragmentation_metric(&self) -> HeapFragmentation {
        let (total_free, largest_free, free_blocks) = unsafe { self.free_list_summary() };

        let (frag_permille, frag_f32) = if total_free == 0 || largest_free >= total_free {
            (0u32, 0.0f32)
        } else {
            let remaining_free = total_free.saturating_sub(largest_free);
            let permille = ((remaining_free as u64).saturating_mul(1000) / total_free as u64) as u32;
            let f32_val = 1.0f32 - (largest_free as f32 / total_free as f32);
            (permille, f32_val)
        };

        HeapFragmentation {
            total_free_bytes: total_free,
            largest_free_block_bytes: largest_free,
            free_blocks_count: free_blocks,
            fragmentation_permille: frag_permille,
            fragmentation_f32: frag_f32,
        }
    }

    /// Allocates memory directly from this allocator instance.
    #[allow(dead_code)]
    pub unsafe fn allocate_raw(&mut self, layout: Layout) -> Option<usize> {
        let (size, align) = Self::size_align(layout);
        self.find_region(size, align)
    }

    /// Deallocates memory directly back to this allocator instance.
    #[allow(dead_code)]
    pub unsafe fn deallocate_raw(&mut self, addr: usize, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        if self.allocated_bytes >= size {
            self.allocated_bytes -= size;
        }
        self.add_free_region(addr, size);
    }

    /// Current allocated bytes.
    #[allow(dead_code)]
    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }

    /// Total heap capacity in bytes.
    #[allow(dead_code)]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

// SAFETY: All mutations to LinkedListAllocator occur under the protection of
// a SpinMutex in `Locked<LinkedListAllocator>`, ensuring thread-safe access.
unsafe impl Send for LinkedListAllocator {}

/// Atomically increments a 64-bit counter using compare-and-swap, saturating at `u64::MAX`.
/// Guarantees that the counter never wraps around to 0 on overflow.
#[inline]
pub fn atomic_saturating_inc(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current == u64::MAX {
            break;
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();

        if let Some(addr) = allocator.find_region(size, align) {
            atomic_saturating_inc(&ALLOC_SUCCESS_COUNT);
            addr as *mut u8
        } else {
            atomic_saturating_inc(&ALLOC_FAIL_COUNT);
            LAST_FAILED_SIZE.store(size, Ordering::Relaxed);
            LAST_FAILED_ALIGN.store(align, Ordering::Relaxed);
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();
        if allocator.allocated_bytes >= size {
            allocator.allocated_bytes -= size;
        }
        allocator.add_free_region(ptr as usize, size);
        atomic_saturating_inc(&DEALLOC_COUNT);
    }
}

/// Global kernel heap allocator registered for the `alloc` crate.
/// Under host unit testing, `global_allocator` registration is omitted to prevent conflict with std.
#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Locked<LinkedListAllocator> = Locked::new(LinkedListAllocator::new());

/// Returns a snapshot of cumulative allocation telemetry statistics.
pub fn allocation_stats() -> AllocatorStats {
    let alloc = ALLOCATOR.lock();
    AllocatorStats {
        success_count: ALLOC_SUCCESS_COUNT.load(Ordering::Relaxed),
        fail_count: ALLOC_FAIL_COUNT.load(Ordering::Relaxed),
        dealloc_count: DEALLOC_COUNT.load(Ordering::Relaxed),
        allocated_bytes: alloc.allocated_bytes,
        total_bytes: alloc.total_bytes,
    }
}

/// Resets allocation telemetry counters (used by test suites).
#[allow(dead_code)]
pub fn reset_allocation_stats() {
    ALLOC_SUCCESS_COUNT.store(0, Ordering::Relaxed);
    ALLOC_FAIL_COUNT.store(0, Ordering::Relaxed);
    DEALLOC_COUNT.store(0, Ordering::Relaxed);
    LAST_FAILED_SIZE.store(0, Ordering::Relaxed);
    LAST_FAILED_ALIGN.store(0, Ordering::Relaxed);
}

/// Returns a safe, non-allocating snapshot of heap diagnostic metrics.
///
/// Retrieves live metrics without performing any heap allocations.
/// Employs `try_lock()` to ensure non-blocking reentrancy safety if called during
/// panic handling or exception processing.
pub fn heap_diagnostics() -> HeapDiagnostics {
    let (total_free, largest_free, free_blocks, alloc_bytes, total_bytes) = match ALLOCATOR.try_lock() {
        Some(alloc) => {
            let (free, largest, count) = unsafe { alloc.free_list_summary() };
            (free, largest, count, alloc.allocated_bytes, alloc.total_bytes)
        }
        None => (0, 0, 0, 0, 0),
    };

    HeapDiagnostics {
        total_bytes,
        allocated_bytes: alloc_bytes,
        total_free_bytes: total_free,
        largest_free_block_bytes: largest_free,
        free_blocks_count: free_blocks,
        success_count: ALLOC_SUCCESS_COUNT.load(Ordering::Relaxed),
        fail_count: ALLOC_FAIL_COUNT.load(Ordering::Relaxed),
        dealloc_count: DEALLOC_COUNT.load(Ordering::Relaxed),
        last_failed_size: LAST_FAILED_SIZE.load(Ordering::Relaxed),
        last_failed_align: LAST_FAILED_ALIGN.load(Ordering::Relaxed),
    }
}

/// Computes the live heap fragmentation metrics without performing any heap allocation.
pub fn heap_fragmentation() -> HeapFragmentation {
    ALLOCATOR.lock().fragmentation_metric()
}

/// Executes an on-demand allocator latency microbenchmark across standard sizes (16B, 64B, 256B, 1KiB, 4KiB).
///
/// Uses x86 `rdtsc` to record raw CPU clock cycles with warmup and median filtering over 5 repetitions.
/// Every allocated block is guaranteed to be deallocated.
/// Reports raw CPU clock cycles; does NOT convert cycles to nanoseconds.
pub fn benchmark_allocator_latency() -> AllocatorBenchmarkReport {
    let frag_before = heap_fragmentation();
    let mut measurements = [
        SizeLatencyMeasurement { size_bytes: 16, alloc_cycles_median: 0, dealloc_cycles_median: 0, repetitions: BENCHMARK_REPETITIONS, success: false },
        SizeLatencyMeasurement { size_bytes: 64, alloc_cycles_median: 0, dealloc_cycles_median: 0, repetitions: BENCHMARK_REPETITIONS, success: false },
        SizeLatencyMeasurement { size_bytes: 256, alloc_cycles_median: 0, dealloc_cycles_median: 0, repetitions: BENCHMARK_REPETITIONS, success: false },
        SizeLatencyMeasurement { size_bytes: 1024, alloc_cycles_median: 0, dealloc_cycles_median: 0, repetitions: BENCHMARK_REPETITIONS, success: false },
        SizeLatencyMeasurement { size_bytes: 4096, alloc_cycles_median: 0, dealloc_cycles_median: 0, repetitions: BENCHMARK_REPETITIONS, success: false },
    ];

    for (idx, &size) in BENCHMARK_SIZES.iter().enumerate() {
        let layout = match Layout::from_size_align(size, 16) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // 1. Warm-up pass to populate CPU caches and warm up free list pointers
        unsafe {
            let warmup_ptr = ALLOCATOR.allocate(layout);
            if !warmup_ptr.is_null() {
                ALLOCATOR.deallocate(warmup_ptr, layout);
            }
        }

        // 2. Timed repetitions
        let mut alloc_samples = [0u64; BENCHMARK_REPETITIONS];
        let mut dealloc_samples = [0u64; BENCHMARK_REPETITIONS];
        let mut all_ok = true;

        for rep in 0..BENCHMARK_REPETITIONS {
            let start_alloc = unsafe { read_cpu_cycles() };
            let ptr = unsafe { ALLOCATOR.allocate(layout) };
            let elapsed_alloc = unsafe { read_cpu_cycles() }.saturating_sub(start_alloc);
            alloc_samples[rep] = elapsed_alloc;

            if ptr.is_null() {
                all_ok = false;
                dealloc_samples[rep] = 0;
            } else {
                let start_dealloc = unsafe { read_cpu_cycles() };
                unsafe { ALLOCATOR.deallocate(ptr, layout) };
                let elapsed_dealloc = unsafe { read_cpu_cycles() }.saturating_sub(start_dealloc);
                dealloc_samples[rep] = elapsed_dealloc;
            }
        }

        alloc_samples.sort_unstable();
        dealloc_samples.sort_unstable();

        measurements[idx] = SizeLatencyMeasurement {
            size_bytes: size,
            alloc_cycles_median: alloc_samples[BENCHMARK_REPETITIONS / 2],
            dealloc_cycles_median: dealloc_samples[BENCHMARK_REPETITIONS / 2],
            repetitions: BENCHMARK_REPETITIONS,
            success: all_ok,
        };
    }

    let frag_after = heap_fragmentation();

    AllocatorBenchmarkReport {
        measurements,
        fragmentation_before: frag_before,
        fragmentation_after: frag_after,
    }
}
