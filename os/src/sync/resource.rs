use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use nalgebra::{DMatrix, DVector};
use crate::sync::UPSafeCell;
use crate::task::RecycleAllocator;

lazy_static! {
    static ref AVAILABLE: UPSafeCell<DVector<usize>> = unsafe {
        UPSafeCell::new(DVector::from_vec(vec![]))
    };
    static ref ALLOCATION: UPSafeCell<DMatrix<usize>> = unsafe {
        UPSafeCell::new(DMatrix::from_vec(0, 0, vec![]))
    };
    static ref NEED: UPSafeCell<DMatrix<usize>> = unsafe {
        UPSafeCell::new(DMatrix::from_vec(0, 0, vec![]))
    };

    static ref RID_ALLOCATOR: UPSafeCell<RecycleAllocator> = unsafe {
        UPSafeCell::new(RecycleAllocator::new())
    };
    static ref RESOURCE_PRODUCERS: UPSafeCell<Vec<Weak<ResourceProducerHandle>>> = unsafe {
        UPSafeCell::new(Vec::new())
    };
}

pub struct ResourceProducerHandle {
    /// Resource ID of this type of resource.
    rid: usize,
    /// Resource amount that this type of resource remains.
    amount: usize,
}

pub struct ResourceHandle {
    /// Thread ID which the resource is possessed by.
    tid: usize,
    /// Resource ID that the thread possesses.
    rid: usize,
    /// Resource amount that the thread possesses.
    amount: usize
}

impl ResourceHandle {
    /// Attempt to allocate amount of resource for thread.
    ///
    /// None is returned if failed to allocate the given amount of
    /// resource for the given thread (which means deadlock might
    /// happen).
    pub fn new(tid: usize, rid: usize, amount: usize) -> Option<Self> {
        // Detect deadlock at first.
        submit_need(tid, rid, amount);
        let deadlock = detect_deadlock();
        remove_need(tid, rid, amount);

        // Can't allocate resource if deadlock is detected.
        if deadlock {
            return None;
        }

        // Allocate the given amount of resource for this thread.
        let available = AVAILABLE.exclusive_access();
        let allocation = ALLOCATION.exclusive_access();
        available[rid] -= amount;
        allocation[(tid, rid)] += amount;
        drop(available);
        drop(allocation);

        // Construct self and return.
        Some(Self { tid, rid, amount })
    }

    /// Attempt to allocate amount of resource with this resource handle.
    ///
    /// False is returned if failed to allocate the given amount of
    /// resource for the given thread (which means deadlock might
    /// happen).
    ///
    /// True is returned if the allocation succeeded.
    pub fn allocate(&mut self, amount: usize) -> bool {
        let tid = self.tid;
        let rid = self.rid;

        // Detect deadlock at first.
        submit_need(tid, rid, amount);
        let deadlock = detect_deadlock();
        remove_need(tid, rid, amount);

        // Can't allocate resource if deadlock is detected.
        if deadlock {
            return false;
        }

        // Allocate the given amount of resource for this thread.
        let available = AVAILABLE.exclusive_access();
        let allocation = ALLOCATION.exclusive_access();
        available[rid] -= amount;
        allocation[(tid, rid)] += amount;
        drop(available);
        drop(allocation);

        // Record this allocation.
        self.amount += amount;

        true
    }

    /// Deallocate amount of resource with this resource handle.
    pub fn deallocate(&mut self, amount: usize) {
        let tid = self.tid;
        let rid = self.rid;

        // Deallocate the given amount of resource for this thread.
        let available = AVAILABLE.exclusive_access();
        let allocation = ALLOCATION.exclusive_access();
        available[rid] += amount;
        allocation[(tid, rid)] -= amount;
        drop(available);
        drop(allocation);

        // Record this deallocation.
        self.amount -= amount;
    }
}

impl Drop for ResourceHandle {
    fn drop(&mut self) {
        // Make sure all amount of resource is deallocated.
        self.deallocate(self.amount);
    }
}

/// Submit need for the given thread before deadlock calculating.
fn submit_need(tid: usize, rid: usize, amount: usize) {
    let need = NEED.exclusive_access();
    need[(tid, rid)] += amount;
    drop(need);
}

/// Remove need for the given thread after deadlock calculating.
fn remove_need(tid: usize, rid: usize, amount: usize) {
    let need = NEED.exclusive_access();
    need[(tid, rid)] -= amount;
    drop(need);
}

/// Detect whether deadlock might happen under the current circumstance.
fn detect_deadlock() -> bool {
    // Get thread count.
    let allocation = ALLOCATION.exclusive_access();
    let thread_count = allocation.row_iter().len();
    // Get resource count.
    let available = AVAILABLE.exclusive_access();
    let resource_count = available.row_iter().len();
    // Initialise Work and Finish Vec.
    let mut work: Vec<usize> = Vec::from_iter(available.iter().map(|x| *x));
    let mut finish: Vec<bool> = Vec::new();

    // Walk through the threads.
    let need = NEED.exclusive_access();
    for i in 0 .. thread_count {
        for j in 0 .. resource_count {
            if need[(i, j)] <= work[j] {
                work[j] += allocation[(i, j)];
                finish[i] = true;
            }
        }
    }

    // Sum up the results to discover whether there is a deadlock.
    let mut no_deadlock = true;
    for v in finish.iter() {
        no_deadlock |= *v;
    }

    !no_deadlock
}