//! Synchronization and interior mutability primitives

mod condvar;
mod mutex;
mod semaphore;
mod up;

pub use condvar::Condvar;
pub use mutex::{Mutex, MutexBlocking, MutexSpin};
pub use semaphore::Semaphore;
pub use up::UPSafeCell;

/// Operation result enum for syncing or locking operations
/// (acquisition of resources).
pub enum OperationResult {
    /// The operation was done without faults.
    Done,
    /// Deadlock is detected and the operation is denied.
    DeadlockDetected
}
