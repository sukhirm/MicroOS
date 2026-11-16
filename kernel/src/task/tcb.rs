//! Task Control Block (TCB) and Task Definitions.
//!
//! Provides the primary task descriptor, state management, dedicated heap-allocated
//! stacks, and initial hardware execution contexts.

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

/// Hardware context layout saved on the task stack during context switch.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TaskContext {
    pub rflags: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64,
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
    /// Creates a new task with an allocated heap stack and initialized stack frame.
    pub fn new(name: &'static str, entry_fn: extern "C" fn()) -> Box<Self> {
        let id = TaskId::new();
        let stack = TaskStack::new(TaskStack::DEFAULT_SIZE);
        let top = stack.top();
        assert!(!top.is_null(), "Task stack top cannot be null");
        assert_eq!(top as usize % 16, 0, "Stack top must be 16-byte aligned");

        // Reserve 8 bytes so that after ret in switch_context, RSP % 16 == 8 on entry to task_trampoline.
        let context_ptr = (top as usize - 8 - core::mem::size_of::<TaskContext>()) as *mut TaskContext;

        unsafe {
            context_ptr.write(TaskContext {
                rflags: 0x2, // reserved bit 1 is always 1 in RFLAGS
                r15: 0,
                r14: 0,
                r13: 0,
                r12: entry_fn as usize as u64,
                rbx: 0,
                rbp: 0,
                rip: super::context::task_trampoline as *const () as usize as u64,
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
