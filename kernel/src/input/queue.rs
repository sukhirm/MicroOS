//! Thread-Safe Bounded Input Event Queue.
//!
//! Provides a lock-protected ring-buffer that safely buffers input events
//! generated inside hardware interrupt handlers (IRQ1, IRQ12) and delivers
//! them to the desktop shell event loop.
//!
//! Invariants:
//! - Non-blocking: `push` never waits or allocates memory.
//! - Overrun safe: When capacity is reached, the oldest event is replaced,
//!   preserving recent user input and incrementing the overrun counter.
//! - Deadlock-free: Uses `IrqGuard` so interrupts are disabled while the queue
//!   lock is held, preventing ISR deadlock against interrupted consumer code.

use super::event::InputEvent;
use crate::sync::irq::IrqGuard;
use crate::sync::mutex::SpinMutex;

/// Maximum number of queued input events before ring-buffer overwrite occurs.
pub const QUEUE_CAPACITY: usize = 128;

/// Internal bounded circular buffer for input events.
pub struct EventQueue {
    buffer: [Option<InputEvent>; QUEUE_CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    overrun_count: usize,
}

#[allow(dead_code)]
impl EventQueue {
    /// Creates a new empty EventQueue.
    pub const fn new() -> Self {
        Self {
            buffer: [None; QUEUE_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            overrun_count: 0,
        }
    }

    /// Pushes an event into the queue. Overwrites the oldest event if full.
    pub fn push(&mut self, event: InputEvent) {
        if self.count == QUEUE_CAPACITY {
            // Buffer full: drop oldest event to make room for fresh input
            self.head = (self.head + 1) % QUEUE_CAPACITY;
            self.overrun_count = self.overrun_count.saturating_add(1);
        } else {
            self.count += 1;
        }

        self.buffer[self.tail] = Some(event);
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
    }

    /// Pops the oldest event from the queue, if available.
    pub fn pop(&mut self) -> Option<InputEvent> {
        if self.count == 0 {
            return None;
        }

        let event = self.buffer[self.head].take();
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.count -= 1;
        event
    }

    /// Returns the number of events currently queued.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns true if the queue contains no events.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the total number of dropped events due to buffer overruns.
    #[inline(always)]
    pub fn overruns(&self) -> usize {
        self.overrun_count
    }

    /// Clears all events from the queue.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
        for slot in &mut self.buffer {
            *slot = None;
        }
    }
}

/// Global system event queue singleton.
static GLOBAL_EVENT_QUEUE: SpinMutex<EventQueue> = SpinMutex::new(EventQueue::new());

/// Publishes an input event to the global queue in an IRQ-safe critical section.
pub fn push_event(event: InputEvent) {
    let _guard = IrqGuard::acquire();
    GLOBAL_EVENT_QUEUE.lock().push(event);
}

/// Pops the next input event from the global queue, returning `None` if empty.
pub fn pop_event() -> Option<InputEvent> {
    let _guard = IrqGuard::acquire();
    GLOBAL_EVENT_QUEUE.lock().pop()
}

/// Returns the number of pending input events in the global queue.
#[allow(dead_code)]
pub fn pending_events() -> usize {
    let _guard = IrqGuard::acquire();
    GLOBAL_EVENT_QUEUE.lock().len()
}

/// Clears the global event queue.
#[allow(dead_code)]
pub fn clear_events() {
    let _guard = IrqGuard::acquire();
    GLOBAL_EVENT_QUEUE.lock().clear();
}

/// Returns the total number of event overruns recorded.
#[allow(dead_code)]
pub fn event_overruns() -> usize {
    let _guard = IrqGuard::acquire();
    GLOBAL_EVENT_QUEUE.lock().overruns()
}
