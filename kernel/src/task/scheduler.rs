//! Preemptive and Cooperative Task Scheduler.
//!
//! Provides thread-safe task management, round-robin scheduling, timer preemption,
//! time-slice tracking, and clean task termination returning to the kernel main thread.

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::mutex::SpinMutex;

use crate::sync::IrqGuard;
use super::queue::ReadyQueue;
use super::tcb::{TaskControlBlock, TaskId, TaskState};

/// Default time slice in timer ticks: 3 ticks (30 ms at 100 Hz).
pub const TIME_SLICE_TICKS: usize = 3;

/// Diagnostic snapshot of a task's state.
#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: TaskId,
    pub name: &'static str,
    pub state: TaskState,
    pub stack_size: usize,
}

/// Global cooperative and preemptive scheduler instance.
pub static SCHEDULER: SpinMutex<Scheduler> = SpinMutex::new(Scheduler::new());

/// The round-robin preemptive scheduler.
pub struct Scheduler {
    ready_queue: ReadyQueue,
    current_task: Option<Box<TaskControlBlock>>,
    main_task: Option<Box<TaskControlBlock>>,
    terminated_tasks: Vec<Box<TaskControlBlock>>,
    current_task_ticks: usize,
    preemption_count: usize,
    context_switch_count: usize,
}

impl Scheduler {
    /// Creates a new empty Scheduler.
    pub const fn new() -> Self {
        Self {
            ready_queue: ReadyQueue::new(),
            current_task: None,
            main_task: None,
            terminated_tasks: Vec::new(),
            current_task_ticks: 0,
            preemption_count: 0,
            context_switch_count: 0,
        }
    }

    /// Spawns a new task and enqueues it to the ready queue.
    pub fn spawn(&mut self, name: &'static str, entry: extern "C" fn()) -> TaskId {
        let task = TaskControlBlock::new(name, entry);
        let id = task.id;
        log::info!(
            "[Scheduler] Spawning task PID: {} ('{}') on 32 KiB heap stack (RSP: {:p})",
            id.as_u64(),
            task.name,
            task.rsp
        );
        self.ready_queue.enqueue(task);
        id
    }

    /// Prepares task termination:
    /// Marks the current task as Terminated, saves it in `terminated_tasks`,
    /// and selects either the next ready task or the main thread to resume.
    pub fn prepare_exit(&mut self) -> *mut u8 {
        let mut current = self.current_task.take().expect("No current task while exiting");
        current.state = TaskState::Terminated;
        log::info!(
            "[Scheduler] Task PID: {} ('{}') marked Terminated.",
            current.id.as_u64(),
            current.name
        );
        self.terminated_tasks.push(current);
        self.context_switch_count += 1;

        if let Some(mut next_task) = self.ready_queue.dequeue() {
            next_task.state = TaskState::Running;
            self.current_task_ticks = 0;
            let next_rsp = next_task.rsp;
            self.current_task = Some(next_task);
            next_rsp
        } else {
            // All worker tasks have terminated; return to main thread
            log::info!("[Scheduler] Ready queue empty. Returning to main kernel thread.");
            let mut main_tcb = self.main_task.take().expect("Main task missing upon scheduler exit");
            main_tcb.state = TaskState::Running;
            self.current_task_ticks = 0;
            let next_rsp = main_tcb.rsp;
            self.current_task = Some(main_tcb);
            next_rsp
        }
    }

    /// Returns the total number of timer-driven preemptive context switches performed.
    #[allow(dead_code)]
    pub fn preemption_count(&self) -> usize {
        self.preemption_count
    }

    /// Returns the currently running task's ID and name, if any.
    pub fn current_task_info(&self) -> Option<(TaskId, &'static str)> {
        self.current_task.as_ref().map(|t| (t.id, t.name))
    }

    /// Returns the total number of context switches recorded.
    pub fn context_switch_count(&self) -> usize {
        self.context_switch_count
    }

    /// Returns the number of tasks in the ready queue.
    pub fn ready_count(&self) -> usize {
        self.ready_queue.len()
    }

    /// Returns the total number of tasks (current, ready, main, and terminated).
    pub fn total_task_count(&self) -> usize {
        (if self.current_task.is_some() { 1 } else { 0 })
            + self.ready_queue.len()
            + (if self.main_task.is_some() { 1 } else { 0 })
            + self.terminated_tasks.len()
    }

    /// Collects a diagnostic snapshot of all tasks in the system.
    pub fn task_snapshots(&self) -> Vec<TaskSnapshot> {
        let mut list = Vec::new();

        if let Some(ref t) = self.current_task {
            list.push(TaskSnapshot {
                id: t.id,
                name: t.name,
                state: TaskState::Running,
                stack_size: t.stack.len(),
            });
        }

        for t in self.ready_queue.iter() {
            list.push(TaskSnapshot {
                id: t.id,
                name: t.name,
                state: TaskState::Ready,
                stack_size: t.stack.len(),
            });
        }

        if let Some(ref t) = self.main_task {
            if self.current_task.as_ref().map(|c| c.id != t.id).unwrap_or(true) {
                list.push(TaskSnapshot {
                    id: t.id,
                    name: t.name,
                    state: t.state,
                    stack_size: t.stack.len(),
                });
            }
        }

        for t in &self.terminated_tasks {
            list.push(TaskSnapshot {
                id: t.id,
                name: t.name,
                state: TaskState::Terminated,
                stack_size: t.stack.len(),
            });
        }

        list
    }
}

/// Spawns a new task onto the global ready queue.
pub fn spawn(name: &'static str, entry: extern "C" fn()) -> TaskId {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().spawn(name, entry)
}

/// Cooperatively yields the remainder of the current task's CPU slice.
#[inline(always)]
pub fn yield_now() {
    unsafe {
        core::arch::asm!("int 48");
    }
}

/// Called by `timer_interrupt_handler` on each hardware PIT tick.
pub fn on_timer_tick(curr_rsp: *mut u8, _ticks: u64) -> *mut u8 {
    let _irq = IrqGuard::acquire();
    let mut sched = SCHEDULER.lock();

    if sched.current_task.is_none() {
        return curr_rsp;
    }

    sched.current_task_ticks += 1;

    // Check if time slice has expired and there is another task ready to run
    if sched.current_task_ticks >= TIME_SLICE_TICKS && !sched.ready_queue.is_empty() {
        sched.preemption_count += 1;
        sched.context_switch_count += 1;
        sched.current_task_ticks = 0;

        let mut curr_task = sched.current_task.take().unwrap();
        curr_task.rsp = curr_rsp;
        curr_task.state = TaskState::Ready;
        sched.ready_queue.enqueue(curr_task);

        let mut next_task = sched.ready_queue.dequeue().unwrap();
        next_task.state = TaskState::Running;
        let next_rsp = next_task.rsp;
        sched.current_task = Some(next_task);

        next_rsp
    } else {
        curr_rsp
    }
}

/// Called by `yield_interrupt_handler` on voluntary cooperative yield (`int 48`).
pub fn on_cooperative_yield(curr_rsp: *mut u8) -> *mut u8 {
    let _irq = IrqGuard::acquire();
    let mut sched = SCHEDULER.lock();

    // If starting multitasking from main thread, save main_task
    if sched.current_task.is_none() && sched.main_task.is_some() {
        if let Some(mut first_task) = sched.ready_queue.dequeue() {
            first_task.state = TaskState::Running;
            let next_rsp = first_task.rsp;
            sched.current_task = Some(first_task);

            let main_task = sched.main_task.as_mut().unwrap();
            main_task.rsp = curr_rsp;
            main_task.state = TaskState::Ready;

            return next_rsp;
        } else {
            return curr_rsp;
        }
    }

    if sched.ready_queue.is_empty() {
        return curr_rsp;
    }

    sched.context_switch_count += 1;
    sched.current_task_ticks = 0;

    let mut curr_task = sched.current_task.take().unwrap();
    curr_task.rsp = curr_rsp;
    curr_task.state = TaskState::Ready;
    sched.ready_queue.enqueue(curr_task);

    let mut next_task = sched.ready_queue.dequeue().unwrap();
    next_task.state = TaskState::Running;
    let next_rsp = next_task.rsp;
    sched.current_task = Some(next_task);

    next_rsp
}

/// Terminates the current task and switches to the next ready task (or main thread).
pub fn exit_current_task() -> ! {
    let next_rsp = {
        let _irq = IrqGuard::acquire();
        let mut scheduler = SCHEDULER.lock();
        scheduler.prepare_exit()
    };

    unsafe {
        super::context::switch_context_exit(next_rsp);
    }
}

/// Starts multitasking from the main thread.
/// Returns once all spawned tasks have completed and terminated.
pub fn start_multitasking() {
    log::info!("[Scheduler] Commencing multitasking switch from main thread...");
    {
        let _irq = IrqGuard::acquire();
        let main_tcb = TaskControlBlock::new_main("main");
        SCHEDULER.lock().main_task = Some(main_tcb);
    }

    // Yield control from main thread to the first ready task
    yield_now();

    // Clean up scheduler state: main thread is no longer managed as a worker task
    {
        let _irq = IrqGuard::acquire();
        SCHEDULER.lock().current_task = None;
    }

    log::info!("[Scheduler] All worker tasks completed. Execution returned to main thread.");
}

/// Returns the total number of timer preemption events recorded.
pub fn preemption_count() -> usize {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().preemption_count()
}

/// Returns the currently running task's ID and name, if any.
pub fn current_task() -> Option<(TaskId, &'static str)> {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().current_task_info()
}

/// Returns the total number of context switches (both preemptive and cooperative).
pub fn context_switch_count() -> usize {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().context_switch_count()
}

/// Returns a diagnostic snapshot of all tasks in the system.
pub fn task_snapshots() -> Vec<TaskSnapshot> {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().task_snapshots()
}

/// Returns the number of tasks in the ready queue.
pub fn ready_task_count() -> usize {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().ready_count()
}

/// Returns the total number of tasks in the system without allocating memory.
pub fn total_task_count() -> usize {
    let _irq = IrqGuard::acquire();
    SCHEDULER.lock().total_task_count()
}
