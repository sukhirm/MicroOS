//! Task Control Block (TCB) and Task Definitions.
//!
//! Provides the primary task descriptor, state management, dedicated heap-allocated
//! stacks, and unified 672-byte hardware/software execution contexts including FPU/SIMD state.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for each task in Micro-OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

impl TaskId {
    /// Generates a monotonically increasing unique TaskId.
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub const fn from_raw(val: u64) -> Self {
        TaskId(val)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Execution state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

/// Dedicated kernel stack allocated from the heap for a task.
pub struct TaskStack {
    memory: Vec<u8>,
}

impl TaskStack {
    /// Default stack size: 32 KiB per task.
    pub const DEFAULT_SIZE: usize = 32 * 1024;

    /// Allocates a new stack on the heap.
    pub fn new(size: usize) -> Self {
        Self {
            memory: alloc::vec![0u8; size],
        }
    }

    /// Creates an empty stack descriptor (used for the initial bootloader task).
    pub fn empty() -> Self {
        Self {
            memory: Vec::new(),
        }
    }

    /// Returns the top of the stack (highest memory address, 16-byte aligned).
    pub fn top(&self) -> *mut u8 {
        if self.memory.is_empty() {
            core::ptr::null_mut()
        } else {
            let bottom = self.memory.as_ptr() as usize;
            let top = bottom + self.memory.len();
            (top & !0xF) as *mut u8
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.memory.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.memory.is_empty()
    }
}

/// 512-byte FPU/MMX/SSE state buffer aligned to 16 bytes for FXSAVE/FXRSTOR.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct FxSaveArea {
    pub data: [u8; 512],
}

impl FxSaveArea {
    /// Creates a clean, standard initial FPU/SSE state for newly created tasks.
    pub fn new_initial() -> Self {
        let mut area = Self { data: [0u8; 512] };
        // fcw (offset 0): 0x037F (all x87 exceptions masked, 64-bit precision, round to nearest)
        area.data[0] = 0x7F;
        area.data[1] = 0x03;
        // mxcsr (offset 24): 0x1F80 (all SIMD exceptions masked, round to nearest)
        area.data[24] = 0x80;
        area.data[25] = 0x1F;
        area
    }
}

/// Unified 672-byte interrupt and context-switch frame.
///
/// Layout:
/// - 0x000..0x1FF (512 bytes): FxSaveArea (x87 FPU, MMX, and SSE state, 16-byte aligned)
/// - 0x200..0x277 (120 bytes): 15 General-Purpose Registers (r15..rax)
/// - 0x278..0x29F (40 bytes): 5 Intel 64 hardware quadwords (RIP, CS, RFLAGS, RSP, SS)
#[repr(C)]
pub struct InterruptContext {
    pub fxsave: FxSaveArea,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Task Control Block representing a schedulable unit of execution.
pub struct TaskControlBlock {
    pub id: TaskId,
    pub name: &'static str,
    pub state: TaskState,
    #[allow(dead_code)]
    pub stack: TaskStack,
    pub rsp: *mut u8,
}

// SAFETY: TaskControlBlock is exclusively managed by the kernel scheduler
// with synchronization protected by spinlocks.
unsafe impl Send for TaskControlBlock {}

impl TaskControlBlock {
    /// Creates a new task with an allocated heap stack and initialized 672-byte frame.
    pub fn new(name: &'static str, entry_fn: extern "C" fn()) -> Box<Self> {
        let id = TaskId::new();
        let stack = TaskStack::new(TaskStack::DEFAULT_SIZE);
        let top = stack.top();
        assert!(!top.is_null(), "Task stack top cannot be null");
        assert_eq!(top as usize % 16, 0, "Stack top must be 16-byte aligned");

        let context_size = core::mem::size_of::<InterruptContext>();
        assert_eq!(context_size, 672, "InterruptContext must be exactly 672 bytes");

        // The context pointer is placed at top - 672. Since top % 16 == 0 and 672 % 16 == 0,
        // context_ptr is guaranteed to be 16-byte aligned for FXSAVE/FXRSTOR.
        let context_ptr = (top as usize - context_size) as *mut InterruptContext;

        unsafe {
            context_ptr.write(InterruptContext {
                fxsave: FxSaveArea::new_initial(),
                r15: 0,
                r14: 0,
                r13: 0,
                r12: entry_fn as *const () as usize as u64,
                r11: 0,
                r10: 0,
                r9: 0,
                r8: 0,
                rdi: 0,
                rsi: 0,
                rbp: 0,
                rbx: 0,
                rdx: 0,
                rcx: 0,
                rax: 0,
                rip: super::context::task_trampoline as *const () as usize as u64,
                cs: 0x08,      // Kernel Code Segment selector (Ring 0)
                rflags: 0x202, // IF=1 (interrupts enabled), reserved bit 1 set
                rsp: top as u64,
                ss: 0x10,      // Kernel Data Segment selector (Ring 0)
            });
        }

        Box::new(Self {
            id,
            name,
            state: TaskState::Ready,
            stack,
            rsp: context_ptr as *mut u8,
        })
    }

    /// Creates a descriptor for the currently running main boot task.
    pub fn new_main(name: &'static str) -> Box<Self> {
        Box::new(Self {
            id: TaskId::from_raw(0),
            name,
            state: TaskState::Running,
            stack: TaskStack::empty(),
            rsp: core::ptr::null_mut(),
        })
    }
}
