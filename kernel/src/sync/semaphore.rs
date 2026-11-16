//! Counting Semaphore.
//!
//! Provides an adaptive cooperative-spinning semaphore backed by atomic CAS operations.
//!
//! # Accurate Architectural Description:
//! This is an adaptive cooperative-spinning semaphore, NOT an OS-level blocking wait-queue
//! semaphore. When contention occurs, the acquiring task cooperatively relinquishes its remaining
//! CPU time slice via `task::yield_now()` with zero locks held, allowing producer tasks to execute
//! and call `release()` without deadlocking the scheduler.

use core::sync::atomic::{AtomicIsize, Ordering};

/// An adaptive cooperative-spinning counting semaphore.
pub struct Semaphore {
    count: AtomicIsize,
}

impl Semaphore {
    /// Creates a new Semaphore with the specified initial count.
    pub const fn new(initial: isize) -> Self {
        Self {
            count: AtomicIsize::new(initial),
        }
    }

    /// Attempts to acquire a unit of the semaphore without waiting.
    ///
    /// Returns `true` if acquired, or `false` if no resource permits were available.
    pub fn try_acquire(&self) -> bool {
        let mut curr = self.count.load(Ordering::Acquire);
        loop {
            if curr <= 0 {
                return false;
            }
            match self.count.compare_exchange_weak(
                curr,
                curr - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }

    /// Acquires a unit of the semaphore.
    ///
    /// If no permits are available, cooperatively yields the CPU to allow producer
    /// tasks to run and release the resource. Zero locks are held while yielding.
    pub fn acquire(&self) {
        while !self.try_acquire() {
            crate::task::yield_now();
        }
    }

    /// Releases a unit of the semaphore, atomically incrementing the permit count.
    pub fn release(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }

    /// Returns the current permit count.
    pub fn count(&self) -> isize {
        self.count.load(Ordering::Relaxed)
    }
}
