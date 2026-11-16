//! FIFO Ready Queue for Task Scheduling.
//!
//! Provides a first-in, first-out queue of runnable tasks waiting for CPU execution.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use super::tcb::TaskControlBlock;

/// FIFO Ready Queue managing runnable tasks.
pub struct ReadyQueue {
    tasks: VecDeque<Box<TaskControlBlock>>,
}

impl ReadyQueue {
    /// Creates an empty ReadyQueue.
    pub const fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }

    /// Enqueues a task to the back of the ready queue.
    pub fn enqueue(&mut self, task: Box<TaskControlBlock>) {
        self.tasks.push_back(task);
    }

    /// Dequeues the next runnable task from the front of the ready queue.
    /// Returns `None` if the queue is empty.
    pub fn dequeue(&mut self) -> Option<Box<TaskControlBlock>> {
        self.tasks.pop_front()
    }

    /// Returns a mutable reference to the task at the back of the queue (if any).
    #[allow(dead_code)]
    pub fn back_mut(&mut self) -> Option<&mut Box<TaskControlBlock>> {
        self.tasks.back_mut()
    }

    /// Returns the number of tasks currently in the ready queue.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Returns true if the ready queue contains no tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}
