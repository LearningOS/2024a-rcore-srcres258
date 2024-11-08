//! Mutex (spin-like and blocking(sleep))

use super::{OperationResult, UPSafeCell};
use crate::task::{current_process, TaskControlBlock};
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self) -> OperationResult;
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
    rid: usize
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        // Allocate a resource id for self.
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let rid = inner.resource_id_allocator.alloc();
        // Set the initial resource amount.
        inner.resource.available[rid] = 1;
        drop(inner);
        
        Self {
            locked: unsafe { UPSafeCell::new(false) },
            rid
        }
    }

    fn submit_need(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.need[tid][self.rid] += 1;
    }

    fn remove_need(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.need[tid][self.rid] -= 1;
    }

    fn alloc_resource(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.allocation[tid][self.rid] += 1;
        inner.resource.available[self.rid] -= 1;
    }

    fn dealloc_resource(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.allocation[tid][self.rid] -= 1;
        inner.resource.available[self.rid] += 1;
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) -> OperationResult {
        trace!("kernel: MutexSpin::lock");
        loop {
            // Get whether deadlock detection is enabled.
            let deadlock_detect = current_process().inner_exclusive_access().deadlock_detect;
            // Submit need of current tid on current rid.
            self.submit_need();
            // Detect deadlock.
            let process = current_process();
            let inner = process.inner_exclusive_access();
            if inner.resource.detect_deadlock() && deadlock_detect {
                // Deadlock is detected and the operation is not forced.
                // Failed to lock the mutex.
                return OperationResult::DeadlockDetected;
            }
            drop(inner);
            drop(process);

            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;

                // Need is satisfied. Remove need and alloc resource.
                self.remove_need();
                self.alloc_resource();

                return OperationResult::Done;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        
        // Deallocate mutex resource for the current thread.
        self.dealloc_resource();
        
        let mut locked = self.locked.exclusive_access();
        *locked = false;
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    rid: usize,
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new() -> Self {
        trace!("kernel: MutexBlocking::new");

        // Allocate a resource id for self.
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let rid = inner.resource_id_allocator.alloc();
        // Set the initial resource amount.
        inner.resource.available[rid] = 1;
        drop(inner);
        
        Self {
            rid,
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new()
                })
            },
        }
    }

    fn submit_need(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.need[tid][self.rid] += 1;
    }

    fn remove_need(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.need[tid][self.rid] -= 1;
    }

    fn alloc_resource(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.allocation[tid][self.rid] += 1;
        inner.resource.available[self.rid] -= 1;
    }

    fn dealloc_resource(&self) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let tid = current_task().unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        inner.resource.allocation[tid][self.rid] -= 1;
        inner.resource.available[self.rid] += 1;
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) -> OperationResult {
        trace!("kernel: MutexBlocking::lock");

        // Get whether deadlock detection is enabled.
        let deadlock_detect = current_process().inner_exclusive_access().deadlock_detect;
        // Submit need of current tid on current rid.
        self.submit_need();
        // Detect deadlock.
        let process = current_process();
        let inner = process.inner_exclusive_access();
        if inner.resource.detect_deadlock() && deadlock_detect {
            // Deadlock is detected and the operation is not forced.
            // Failed to lock the mutex.
            return OperationResult::DeadlockDetected;
        }
        drop(inner);
        drop(process);

        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
        }

        // Need is satisfied. Remove need and alloc resource.
        self.remove_need();
        self.alloc_resource();

        OperationResult::Done
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            wakeup_task(waking_task);
        } else {
            // Deallocate mutex resource for the current thread.
            self.dealloc_resource();
            
            mutex_inner.locked = false;
        }
    }
}
