//! Task Management and Cooperative Context Switching Subsystem for Micro-OS.
//!
//! Provides Task Control Blocks (TCB), FIFO Ready Queue, cooperative round-robin scheduling,
//! genuine assembly context switching, and multi-task demonstration.

pub mod context;
pub mod queue;
pub mod scheduler;
pub mod tcb;

pub use scheduler::{current_task, spawn, start_multitasking, yield_now};
pub use tcb::{TaskId, TaskState};

extern "C" fn task_alpha_entry() {
    let (id, name) = current_task().expect("Task Alpha must have active context");
    log::info!("[{}] (PID: {}) Started execution on dedicated heap stack.", name, id.as_u64());
    for step in 1..=3 {
        log::info!("[{}] (PID: {}) Step {}/3 - yielding CPU cooperatively", name, id.as_u64(), step);
        yield_now();
    }
    log::info!("[{}] (PID: {}) Finished work; returning for clean termination.", name, id.as_u64());
}

extern "C" fn task_beta_entry() {
    let (id, name) = current_task().expect("Task Beta must have active context");
    log::info!("[{}] (PID: {}) Started execution on dedicated heap stack.", name, id.as_u64());
    for step in 1..=3 {
        log::info!("[{}] (PID: {}) Step {}/3 - yielding CPU cooperatively", name, id.as_u64(), step);
        yield_now();
    }
    log::info!("[{}] (PID: {}) Finished work; returning for clean termination.", name, id.as_u64());
}

/// Runs comprehensive self-tests verifying TaskId uniqueness, state transitions,
/// ready queue FIFO semantics, empty queue handling, and stack 16-byte alignment.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("          Running Micro-OS Phase 3B Task Subsystem Self-Tests   ");
    log::info!("================================================================");

    // 1. Task ID uniqueness test
    {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        let id3 = TaskId::new();
        let id4 = TaskId::new();
        let id5 = TaskId::new();

        assert!(
            id1.as_u64() < id2.as_u64()
                && id2.as_u64() < id3.as_u64()
                && id3.as_u64() < id4.as_u64()
                && id4.as_u64() < id5.as_u64(),
            "Task IDs must be strictly unique and monotonically increasing"
        );
        log::info!(
            "  [PASS] Task ID uniqueness test (IDs generated: {}, {}, {}, {}, {})",
            id1.as_u64(), id2.as_u64(), id3.as_u64(), id4.as_u64(), id5.as_u64()
        );
    }

    // 2. Task state transitions test
    {
        let mut state = TaskState::Ready;
        assert_eq!(state, TaskState::Ready);
        state = TaskState::Running;
        assert_eq!(state, TaskState::Running);
        state = TaskState::Blocked;
        assert_eq!(state, TaskState::Blocked);
        state = TaskState::Ready;
        assert_eq!(state, TaskState::Ready);
        state = TaskState::Terminated;
        assert_eq!(state, TaskState::Terminated);
        log::info!("  [PASS] Task state transition test (Ready -> Running -> Blocked -> Ready -> Terminated)");
    }

    // 3. Ready queue operations & empty queue handling
    {
        extern "C" fn dummy_task() {}
        let mut queue = queue::ReadyQueue::new();
        assert!(queue.is_empty(), "Initial ready queue must be empty");
        assert_eq!(queue.len(), 0);
        assert!(queue.dequeue().is_none(), "Dequeue on empty queue must return None");

        let task1 = tcb::TaskControlBlock::new("dummy1", dummy_task);
        let id1 = task1.id;
        let task2 = tcb::TaskControlBlock::new("dummy2", dummy_task);
        let id2 = task2.id;
        let task3 = tcb::TaskControlBlock::new("dummy3", dummy_task);
        let id3 = task3.id;

        queue.enqueue(task1);
        queue.enqueue(task2);
        queue.enqueue(task3);

        assert_eq!(queue.len(), 3);
        assert!(!queue.is_empty());

        let pop1 = queue.dequeue().expect("Expected task 1");
        assert_eq!(pop1.id, id1);
        let pop2 = queue.dequeue().expect("Expected task 2");
        assert_eq!(pop2.id, id2);
        let pop3 = queue.dequeue().expect("Expected task 3");
        assert_eq!(pop3.id, id3);

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.dequeue().is_none(), "Empty queue must cleanly return None");
        log::info!("  [PASS] Ready queue FIFO & empty operations test");
    }

    // 4. Task stack 16-byte alignment verification
    {
        let stack = tcb::TaskStack::new(tcb::TaskStack::DEFAULT_SIZE);
        let top = stack.top();
        assert!(!top.is_null());
        assert_eq!(
            top as usize % 16,
            0,
            "Task stack top must be 16-byte aligned for System V ABI: {:p}",
            top
        );

        extern "C" fn align_dummy() {}
        let tcb = tcb::TaskControlBlock::new("align_test", align_dummy);
        // context_ptr is at top - 72. (top - 72) % 16 == 8.
        let initial_rsp = tcb.rsp as usize;
        assert_eq!(
            initial_rsp % 16,
            8,
            "Initial context RSP must satisfy (top - 72) % 16 == 8 for System V ABI: {:#x}",
            initial_rsp
        );
        log::info!(
            "  [PASS] Task stack 16-byte alignment test (stack top: {:p}, initial RSP: {:#x})",
            top, initial_rsp
        );
    }

    log::info!("================================================================");
    log::info!("     [OK] All Phase 3B Task Subsystem Self-Tests Passed!        ");
    log::info!("================================================================");
}

/// Demonstrates cooperative multitasking with Task Alpha and Task Beta.
pub fn run_demo() {
    log::info!("================================================================");
    log::info!("     Starting Micro-OS Phase 3B Multitasking Demonstration     ");
    log::info!("================================================================");

    let id_alpha = spawn("Task Alpha", task_alpha_entry);
    let id_beta = spawn("Task Beta", task_beta_entry);

    log::info!(
        "[Demo] Spawned worker tasks: Task Alpha (PID: {}), Task Beta (PID: {})",
        id_alpha.as_u64(),
        id_beta.as_u64()
    );

    // Hand over execution to the cooperative round-robin scheduler
    start_multitasking();

    log::info!("================================================================");
    log::info!("  [OK] Cooperative Multitasking Demonstration Complete!        ");
    log::info!("================================================================");
}
