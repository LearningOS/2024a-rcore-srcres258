//! Synchronization and interior mutability primitives

mod condvar;
mod mutex;
mod semaphore;
mod up;
mod resource;

pub use condvar::Condvar;
pub use mutex::{Mutex, MutexBlocking, MutexSpin};
pub use semaphore::Semaphore;
pub use up::UPSafeCell;
pub use resource::{
    ResourceProducerHandle,
    ResourceConsumerHandle,
    ResourceConsumerHandleCollection,
    OperationResult,
    detect_deadlock
};
