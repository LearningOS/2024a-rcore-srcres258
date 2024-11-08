//! Mutex (spin-like and blocking(sleep))

use super::{OperationResult, ResourceProducerHandle, UPSafeCell};
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self, force: bool) -> OperationResult;
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
    handle: UPSafeCell<Arc<ResourceProducerHandle>>
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
            handle: unsafe { UPSafeCell::new(ResourceProducerHandle::new(1)) }
        }
    }

    /// If deadlock is detected, return false.
    fn alloc_resource(&self, force: bool) -> bool {
        let handle = self.handle.exclusive_access();
        let rid = handle.rid();
        drop(handle);
        let task = current_task().unwrap();
        let result = task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .allocate(rid, 1);
        drop(task);

        result || force
    }

    fn dealloc_resource(&self) {
        let handle = self.handle.exclusive_access();
        let rid = handle.rid();
        drop(handle);
        let task = current_task().unwrap();
        task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .deallocate(rid, 1);
        drop(task);
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self, force: bool) -> OperationResult {
        trace!("kernel: MutexSpin::lock");
        loop {
            // Try to allocate mutex resource for the current thread.
            if !self.alloc_resource(force) {
                // Deadlock is detected and the operation is not forced.
                // Failed to lock the mutex.
                return OperationResult::DeadlockDetected;
            }

            let mut locked = self.locked.exclusive_access();
            if *locked {
                self.dealloc_resource();
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
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
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
    handle: Arc<ResourceProducerHandle>
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new() -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                    handle: ResourceProducerHandle::new(1)
                })
            },
        }
    }

    /// If deadlock is detected, return false.
    fn alloc_resource(&self, force: bool) -> bool {
        let mutex_inner = self.inner.exclusive_access();

        // Allocate mutex resource for the current thread.
        let rid = mutex_inner.handle.rid();
        let task = current_task().unwrap();
        let result = task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .allocate(rid, 1);
        drop(task);

        result || force
    }

    fn dealloc_resource(&self) {
        let mutex_inner = self.inner.exclusive_access();

        let rid = mutex_inner.handle.rid();
        let task = current_task().unwrap();
        task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .deallocate(rid, 1);
        drop(task);
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self, force: bool) -> OperationResult {
        trace!("kernel: MutexBlocking::lock");

        // Try to allocate mutex resource for the current thread.
        if !self.alloc_resource(force) {
            // Deadlock is detected and the operation is not forced.
            // Failed to lock the mutex.
            return OperationResult::DeadlockDetected;
        }

        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            drop(mutex_inner);
            self.dealloc_resource();
            let mut mutex_inner = self.inner.exclusive_access();
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
        }

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
            drop(mutex_inner);
            self.dealloc_resource();
            let mut mutex_inner = self.inner.exclusive_access();
            
            mutex_inner.locked = false;
        }
    }
}
