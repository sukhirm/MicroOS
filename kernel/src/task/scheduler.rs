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
        core::arch::asm!("int 33");
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

/// Called by `yield_interrupt_handler` on voluntary cooperative yield (`int 33`).
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
