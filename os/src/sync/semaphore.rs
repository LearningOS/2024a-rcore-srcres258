//! Semaphore

use crate::sync::{OperationResult, UPSafeCell};
use crate::task::{block_current_and_run_next, current_process, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    rid: usize,
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        
        // Allocate a resource id for self.
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let rid = inner.resource_id_allocator.alloc();
        // Set the initial resource amount.
        inner.resource.available[rid] = res_count;
        drop(inner);
        
        Self {
            rid,
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
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
    pub fn down(&self) -> OperationResult {
        trace!("kernel: Semaphore::down");

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
        
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        }
        
        // Need is satisfied. Remove need and alloc resource.
        self.remove_need();
        self.alloc_resource();

        OperationResult::Done
    }
}
