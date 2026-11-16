//! Mutual Exclusion Spinlock (`SpinMutex`).
//!
//! Provides a mutual exclusion primitive based on atomic test-and-set.
//!
//! # Invariants:
//! - Tasks MUST NOT voluntarily call `task::yield_now()` while holding a `SpinMutex`.
//! - Critical sections should be brief and non-blocking.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A mutual exclusion primitive based on atomic spin-locking.
pub struct SpinMutex<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Sync for SpinMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    /// Creates a new unlocked SpinMutex wrapping `data`.
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> SpinMutex<T> {
    /// Attempts to acquire the lock without spinning.
    /// Returns a guard if successful, or None if the lock was already held.
    pub fn try_lock(&self) -> Option<SpinMutexGuard<'_, T>> {
        if self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinMutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Acquires the spinlock, spinning until the lock becomes available.
    ///
    /// Note: The caller MUST NOT yield while holding the returned guard.
    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        loop {
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return SpinMutexGuard { mutex: self };
            }
            core::hint::spin_loop();
        }
    }

    /// Checks whether the lock is currently held.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Releases the lock.
    unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

/// An RAII guard providing scoped mutable access to the protected data.
pub struct SpinMutexGuard<'a, T: ?Sized> {
    mutex: &'a SpinMutex<T>,
}

impl<T: ?Sized> Deref for SpinMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for SpinMutexGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            self.mutex.unlock();
        }
    }
}
