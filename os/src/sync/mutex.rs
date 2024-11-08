//! Mutex (spin-like and blocking(sleep))

use super::{ResourceProducerHandle, UPSafeCell};
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self);
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
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) {
        trace!("kernel: MutexSpin::lock");
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                // Allocate mutex resource for the current thread.
                let handle = self.handle.exclusive_access();
                let rid = handle.rid();
                drop(handle);
                let task = current_task().unwrap();
                task.inner_exclusive_access()
                    .res
                    .as_mut()
                    .unwrap()
                    .resource_handles
                    .allocate(rid, 1); // TODO: deadlock detection
                drop(task);
                
                *locked = true;
                return;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        
        // Deallocate mutex resource for the current thread.
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
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) {
        trace!("kernel: MutexBlocking::lock");
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            // Allocate mutex resource for the current thread.
            let rid = mutex_inner.handle.rid();
            let task = current_task().unwrap();
            task.inner_exclusive_access()
                .res
                .as_mut()
                .unwrap()
                .resource_handles
                .allocate(rid, 1); // TODO: deadlock detection
            drop(task);
            
            mutex_inner.locked = true;
        }
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
            let rid = mutex_inner.handle.rid();
            let task = current_task().unwrap();
            task.inner_exclusive_access()
                .res
                .as_mut()
                .unwrap()
                .resource_handles
                .deallocate(rid, 1);
            drop(task);
            
            mutex_inner.locked = false;
        }
    }
}
