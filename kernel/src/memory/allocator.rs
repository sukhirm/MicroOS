//! Kernel Heap Allocator.
//!
//! Provides a thread-safe linked-list allocator implementing `core::alloc::GlobalAlloc`.
//! Manages dynamic heap allocations, splits free blocks, and coalesces adjacent regions.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use spin::mutex::SpinMutex;

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
        self.add_free_region(heap_start, heap_size);
    }

    /// Adds a freed memory region back into the address-sorted free list.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
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
    unsafe fn coalesce(&mut self) {
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
    unsafe fn find_region(&mut self, size: usize, align: usize) -> Option<usize> {
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
    fn size_align(layout: Layout) -> (usize, usize) {
        let align = layout.align().max(16);
        let size = ((layout.size().max(16) + 15) / 16) * 16;
        (size, align)
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

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();

        if let Some(addr) = allocator.find_region(size, align) {
            addr as *mut u8
        } else {
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
    }
}

/// Global kernel heap allocator registered for the `alloc` crate.
#[global_allocator]
pub static ALLOCATOR: Locked<LinkedListAllocator> = Locked::new(LinkedListAllocator::new());
