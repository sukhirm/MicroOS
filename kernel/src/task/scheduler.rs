//! Cooperative Task Scheduler.
//!
//! Provides thread-safe task management, round-robin scheduling, task yielding,
//! and clean task termination returning to the kernel main thread.

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::mutex::SpinMutex;

use super::queue::ReadyQueue;
use super::tcb::{TaskControlBlock, TaskId, TaskState};

/// Global cooperative scheduler instance.
pub static SCHEDULER: SpinMutex<Scheduler> = SpinMutex::new(Scheduler::new());

/// The cooperative round-robin scheduler.
pub struct Scheduler {
    ready_queue: ReadyQueue,
    current_task: Option<Box<TaskControlBlock>>,
    main_task: Option<Box<TaskControlBlock>>,
    terminated_tasks: Vec<Box<TaskControlBlock>>,
}

impl Scheduler {
    /// Creates a new empty Scheduler.
    pub const fn new() -> Self {
        Self {
            ready_queue: ReadyQueue::new(),
            current_task: None,
            main_task: None,
            terminated_tasks: Vec::new(),
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

    /// Prepares to commence multitasking by saving the main thread descriptor
    /// and selecting the first task from the ready queue.
    pub fn start(&mut self) -> Option<(*mut *mut u8, *mut u8)> {
        let mut first_task = self.ready_queue.dequeue()?;
        first_task.state = TaskState::Running;
        let next_rsp = first_task.rsp;

        let main_tcb = TaskControlBlock::new_main("main");
        self.main_task = Some(main_tcb);
        self.current_task = Some(first_task);

        let prev_rsp_ptr = &mut self.main_task.as_mut().unwrap().rsp as *mut *mut u8;
        Some((prev_rsp_ptr, next_rsp))
    }

    /// Prepares cooperative round-robin yield:
    /// Returns the previous task's RSP storage pointer and the next task's target RSP.
    pub fn prepare_yield(&mut self) -> Option<(*mut *mut u8, *mut u8)> {
        if self.ready_queue.is_empty() {
            // No other tasks waiting; current task continues execution.
            return None;
        }

        let mut next_task = self.ready_queue.dequeue()?;
        next_task.state = TaskState::Running;

        let mut prev_task = self.current_task.take().expect("No current task while yielding");
        prev_task.state = TaskState::Ready;

        self.ready_queue.enqueue(prev_task);
        self.current_task = Some(next_task);

        // prev_task is now at the back of ready_queue (its heap address is stable)
        let prev_rsp_ptr = &mut self.ready_queue.back_mut().unwrap().rsp as *mut *mut u8;
        let next_rsp = self.current_task.as_ref().unwrap().rsp;

        Some((prev_rsp_ptr, next_rsp))
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
            let next_rsp = next_task.rsp;
            self.current_task = Some(next_task);
            next_rsp
        } else {
            // All worker tasks have terminated; return to main thread
            log::info!("[Scheduler] Ready queue empty. Returning to main kernel thread.");
            let mut main_tcb = self.main_task.take().expect("Main task missing upon scheduler exit");
            main_tcb.state = TaskState::Running;
            let next_rsp = main_tcb.rsp;
            self.current_task = Some(main_tcb);
            next_rsp
        }
    }

    /// Returns the currently running task's ID and name, if any.
    #[allow(dead_code)]
    pub fn current_task_info(&self) -> Option<(TaskId, &'static str)> {
        self.current_task.as_ref().map(|t| (t.id, t.name))
    }
}

/// Returns the currently running task's ID and name, if any.
pub fn current_task() -> Option<(TaskId, &'static str)> {
    SCHEDULER.lock().current_task_info()
}

/// Spawns a new task onto the global ready queue.
pub fn spawn(name: &'static str, entry: extern "C" fn()) -> TaskId {
    SCHEDULER.lock().spawn(name, entry)
}

/// Cooperatively yields the remainder of the current task's CPU slice.
pub fn yield_now() {
    let switch_data = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.prepare_yield()
    };

    if let Some((prev_rsp_ptr, next_rsp)) = switch_data {
        // SAFETY: Both prev_rsp_ptr and next_rsp point to verified valid stack frames,
        // and SCHEDULER lock has been cleanly released.
        unsafe {
            super::context::switch_context(prev_rsp_ptr, next_rsp);
        }
    }
}

/// Terminates the current task and switches to the next ready task (or main thread).
pub fn exit_current_task() -> ! {
    let next_rsp = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.prepare_exit()
    };

    // SAFETY: next_rsp points to a verified valid saved context; outgoing stack is discarded.
    unsafe {
        super::context::switch_context_exit(next_rsp);
    }
}

/// Starts cooperative multitasking from the main thread.
/// Returns once all spawned tasks have completed and terminated.
pub fn start_multitasking() {
    let switch_data = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.start()
    };

    if let Some((prev_rsp_ptr, next_rsp)) = switch_data {
        log::info!("[Scheduler] Commencing cooperative multitasking switch from main thread...");
        unsafe {
            super::context::switch_context(prev_rsp_ptr, next_rsp);
        }
        log::info!("[Scheduler] All worker tasks completed. Execution returned to main thread.");
    } else {
        log::warn!("[Scheduler] No runnable tasks in ready queue to start.");
    }
}
