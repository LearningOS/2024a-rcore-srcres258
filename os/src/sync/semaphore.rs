//! Semaphore

use crate::sync::{detect_deadlock, OperationResult, ResourceProducerHandle, UPSafeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
    handle: Arc<ResourceProducerHandle>
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                    handle: ResourceProducerHandle::new(res_count)
                })
            },
        }
    }

    fn submit_need(&self) {
        let inner = self.inner.exclusive_access();

        // Submit need for the semaphore on current tid.
        let rid = inner.handle.rid();
        let task = current_task().unwrap();
        task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .submit_need(rid, 1);
    }

    fn remove_need(&self) {
        let inner = self.inner.exclusive_access();

        // Remove need for the semaphore on current tid.
        let rid = inner.handle.rid();
        let task = current_task().unwrap();
        task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .remove_need(rid, 1);
    }

    fn alloc_resource(&self, force: bool) -> bool {
        let inner = self.inner.exclusive_access();

        let rid = inner.handle.rid();
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
        let inner = self.inner.exclusive_access();

        let rid = inner.handle.rid();
        let task = current_task().unwrap();
        task.inner_exclusive_access()
            .res
            .as_mut()
            .unwrap()
            .resource_handles
            .deallocate(rid, 1);
        drop(task);
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        
        // Deallocate semaphore resource for the current thread.
        self.dealloc_resource();
        
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self, force: bool) -> OperationResult {
        trace!("kernel: Semaphore::down");

        // Submit need of current tid on current rid.
        self.submit_need();
        // Detect deadlock.
        if detect_deadlock() && !force {
            // Deadlock is detected and the operation is not forced.
            // Failed to make semaphore down.
            return OperationResult::DeadlockDetected;
        }
        
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        } else {
            drop(inner);
        } // inner has been completely dropped here.

        // Need is satisfied. Remove need and alloc resource.
        self.remove_need();
        self.alloc_resource(force);

        OperationResult::Done
    }
}
