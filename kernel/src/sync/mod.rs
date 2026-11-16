//! Concurrency Synchronization Primitives for Micro-OS.
//!
//! Provides atomic IRQ guards (`IrqGuard`), mutual exclusion spinlocks (`SpinMutex`),
//! and an adaptive cooperative-spinning counting semaphore (`Semaphore`).

pub mod irq;
pub mod mutex;
pub mod semaphore;

pub use irq::IrqGuard;
#[allow(unused_imports)]
pub use mutex::{SpinMutex, SpinMutexGuard};
pub use semaphore::Semaphore;

/// Runs self-tests for synchronization primitives.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("       Running Micro-OS Phase 3C Synchronization Self-Tests     ");
    log::info!("================================================================");

    // 1. IrqGuard nesting & atomic state capture test
    {
        // Inside kernel startup with interrupts disabled (IF = 0)
        let guard1 = IrqGuard::acquire();
        assert!(!guard1.was_interrupt_enabled(), "Initial boot IF must be 0");
        {
            let guard2 = IrqGuard::acquire();
            assert!(!guard2.was_interrupt_enabled(), "Nested guard IF must be 0");
        }
        // After inner drop, interrupts must still be disabled
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq", "pop {0}", out(reg) flags);
        }
        assert_eq!(flags & 0x200, 0, "Interrupts must remain disabled after nested drop");
        drop(guard1);
        log::info!("  [PASS] IrqGuard atomic state capture and nested preservation test");
    }

    // 2. SpinMutex lock, try_lock, and mutual exclusion test
    {
        let mutex = SpinMutex::new(42u64);
        assert!(!mutex.is_locked());

        {
            let mut guard = mutex.lock();
            assert!(mutex.is_locked());
            assert_eq!(*guard, 42);
            *guard = 100;
            assert!(mutex.try_lock().is_none(), "try_lock on held mutex must return None");
        }

        assert!(!mutex.is_locked());
        let final_val = mutex.lock();
        assert_eq!(*final_val, 100);
        log::info!("  [PASS] SpinMutex mutual exclusion and try_lock contention test");
    }

    // 3. Semaphore capacity bounding and atomic operations test
    {
        let sem = Semaphore::new(2);
        assert_eq!(sem.count(), 2);

        assert!(sem.try_acquire());
        assert_eq!(sem.count(), 1);
        assert!(sem.try_acquire());
        assert_eq!(sem.count(), 0);

        // When capacity is 0, try_acquire must return false without waiting
        assert!(!sem.try_acquire());

        sem.release();
        assert_eq!(sem.count(), 1);
        assert!(sem.try_acquire());
        assert_eq!(sem.count(), 0);

        sem.release();
        sem.release();
        assert_eq!(sem.count(), 2);
        log::info!("  [PASS] Semaphore bounded capacity and atomic acquire/release test");
    }

    log::info!("================================================================");
    log::info!("   [OK] All Phase 3C Synchronization Self-Tests Passed!        ");
    log::info!("================================================================");
}
