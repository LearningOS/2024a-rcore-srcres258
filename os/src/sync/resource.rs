use alloc::sync::{Arc, Weak};
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
    static ref RESOURCE_PRODUCERS: UPSafeCell<Vec<Option<Weak<ResourceProducerHandle>>>> = unsafe {
        UPSafeCell::new(Vec::new())
    };
}

pub struct ResourceProducerHandle {
    /// Resource ID of this type of resource.
    rid: usize,
    /// Resource initial amount of this type of resource.
    /// This should be immutable after initialisation.
    initial_amount: usize,
    /// Resource amount that this type of resource remains.
    amount: usize,
}

pub struct ResourceConsumerHandle {
    /// Thread ID which the resource is possessed by.
    tid: usize,
    /// Resource ID that the thread possesses.
    rid: usize,
    /// Resource amount that the thread possesses.
    amount: usize
}

pub struct ResourceConsumerHandleCollection {
    /// Thread ID which the resources within this collection are possessed by.
    tid: usize,
    /// Consumer resource handles possessed by this collection.
    handles: Vec<ResourceConsumerHandle>
}

impl ResourceProducerHandle {
    pub fn new(initial_amount: usize) -> Arc<Self> {
        // Allocate a rid for this type of resource.
        let mut rid_allocator = RID_ALLOCATOR.exclusive_access();
        let rid = rid_allocator.alloc();
        drop(rid_allocator);
        // Set value of self in AVAILABLE vector to initial amount.
        adjust_available_vec_len(rid);
        let available = AVAILABLE.exclusive_access();
        available[rid] = initial_amount;
        drop(available);
        // Construct self.
        let result = Arc::new(Self {
            rid,
            initial_amount,
            amount: initial_amount
        });
        // Mark self in RESOURCE_PRODUCERS.
        set_resource_producer(rid, Some(Arc::downgrade(&result)));

        result
    }

    /// Allocate amount of this type of resource **without**
    /// deadlock detection.
    ///
    /// This method should only be called by ResourceProducerHandle
    /// during its resource allocation process.
    fn allocate(&mut self, amount: usize) {
        let rid = self.rid;

        adjust_available_vec_len(rid);
        let available = AVAILABLE.exclusive_access();
        available[rid] -= amount;

        self.amount -= amount;
    }

    /// Deallocate amount of this type of resource.
    ///
    /// This method should only be called by ResourceProducerHandle
    /// during its resource allocation process.
    fn deallocate(&mut self, amount: usize) {
        let rid = self.rid;

        adjust_available_vec_len(rid);
        let available = AVAILABLE.exclusive_access();
        available[rid] -= amount;

        self.amount += amount;
    }

    #[allow(unused)]
    pub fn rid(&self) -> usize {
        self.rid
    }

    #[allow(unused)]
    pub fn initial_amount(self) -> usize {
        self.initial_amount
    }

    #[allow(unused)]
    pub fn amount(&self) -> usize {
        self.amount
    }
}

impl Drop for ResourceProducerHandle {
    fn drop(&mut self) {
        // Set value of self in AVAILABLE vector to zero.
        adjust_available_vec_len(self.rid);
        let available = AVAILABLE.exclusive_access();
        available[self.rid] = 0;
        drop(available);
        // Deallocate self's rid.
        let mut rid_allocator = RID_ALLOCATOR.exclusive_access();
        rid_allocator.dealloc(self.rid);
    }
}

impl ResourceConsumerHandle {
    /// Attempt to allocate amount of resource for thread.
    ///
    /// None is returned if failed to allocate the given amount of
    /// resource for the given thread (which means deadlock might
    /// happen).
    pub fn new(tid: usize, rid: usize, amount: usize) -> Option<Self> {
        // Check if this type of resource exists.
        if !resource_producer_exists(rid) {
            return None;
        }

        // Detect deadlock at first.
        submit_need(tid, rid, amount);
        let deadlock = detect_deadlock();
        remove_need(tid, rid, amount);

        // Can't allocate resource if deadlock is detected.
        if deadlock {
            return None;
        }

        // Call into ResourceProducerHandle for resource allocation marking.
        let producers = RESOURCE_PRODUCERS.exclusive_access();
        let mut producer = producers.get(rid).unwrap()
            .as_ref().unwrap().upgrade().unwrap();
        producer.allocate(amount);
        drop(producer);
        drop(producers);

        // Mark the given amount of resource is allocated for this thread.
        adjust_allocation_mat_size(tid, rid);
        let allocation = ALLOCATION.exclusive_access();
        allocation[(tid, rid)] += amount;
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

        // Check if this type of resource exists.
        if !resource_producer_exists(rid) {
            return false;
        }

        // Detect deadlock at first.
        submit_need(tid, rid, amount);
        let deadlock = detect_deadlock();
        remove_need(tid, rid, amount);

        // Can't allocate resource if deadlock is detected.
        if deadlock {
            return false;
        }

        // Call into ResourceProducerHandle for resource allocation marking.
        let producers = RESOURCE_PRODUCERS.exclusive_access();
        let mut producer = producers.get(rid).unwrap()
            .as_ref().unwrap().upgrade().unwrap();
        producer.allocate(amount);
        drop(producer);
        drop(producers);

        // Mark the given amount of resource is allocated for this thread.
        adjust_allocation_mat_size(tid, rid);
        let allocation = ALLOCATION.exclusive_access();
        allocation[(tid, rid)] += amount;
        drop(allocation);

        // Record this allocation.
        self.amount += amount;

        true
    }

    /// Deallocate amount of resource with this resource handle.
    pub fn deallocate(&mut self, amount: usize) {
        let tid = self.tid;
        let rid = self.rid;

        // Check if this type of resource exists.
        if !resource_producer_exists(rid) {
            // If this type of resource isn't existing now,
            // this resource consumer handle is illegal at present,
            // so we just need to set value in ALLOCATION to zero.
            adjust_allocation_mat_size(tid, rid);
            let allocation = ALLOCATION.exclusive_access();
            allocation[(tid, rid)] = 0;
        } else {
            // Call into ResourceProducerHandle for resource deallocation marking.
            let producers = RESOURCE_PRODUCERS.exclusive_access();
            let mut producer = producers.get(rid).unwrap()
                .as_ref().unwrap().upgrade().unwrap();
            producer.deallocate(amount);
            drop(producer);
            drop(producers);

            // Mark the given amount of resource is deallocated for this thread.
            adjust_allocation_mat_size(tid, rid);
            let allocation = ALLOCATION.exclusive_access();
            allocation[(tid, rid)] -= amount;
            drop(allocation);
        }

        // Record this deallocation.
        self.amount -= amount;
    }

    #[allow(unused)]
    pub fn tid(&self) -> usize {
        self.tid
    }

    #[allow(unused)]
    pub fn rid(&self) -> usize {
        self.rid
    }

    #[allow(unused)]
    pub fn amount(&self) -> usize {
        self.amount
    }
}

impl Drop for ResourceConsumerHandle {
    fn drop(&mut self) {
        // Make sure all amount of resource is deallocated.
        self.deallocate(self.amount);
    }
}

impl ResourceConsumerHandleCollection {
    pub fn new(tid: usize) -> ResourceConsumerHandleCollection {
        Self {
            tid,
            handles: Vec::new()
        }
    }

    /// Attempt to allocate amount of the given resource within this collection.
    ///
    /// If failed to allocate (deadlock is detected), false is returned.
    pub fn allocate(&mut self, rid: usize, amount: usize) -> bool {
        // Check whether this type of resource has been allocated.
        let mut handle = None;
        for h in self.handles.iter_mut() {
            if h.rid == rid {
                handle = Some(h);
                break;
            }
        }
        if let Some(h) = handle {
            // If handle is allocated, allocate the new amount with this handle.
            h.allocate(amount)
        } else {
            // If handle isn't allocated, allocate a new resource handle.
            let handle = ResourceConsumerHandle::new(self.tid, rid, amount);

            if let Some(h) = handle {
                // Put the handle into self.
                self.handles.push(h);

                true
            } else {
                // Failed to allocate amount of this resource.
                false
            }
        }
    }

    /// Attempt to deallocate amount of the given resource within this collection.
    ///
    /// If the resource does not exist in this collection, false is returned.
    pub fn deallocate(&mut self, rid: usize, amount: usize) -> bool {
        for h in self.handles.iter_mut() {
            if h.rid == rid {
                h.deallocate(amount);
                return true;
            }
        }

        false
    }
}

/// Adjust len of RESOURCE_PRODUCERS if it is not long enough.
fn adjust_resource_producers_len(rid: usize) {
    let mut producers = RESOURCE_PRODUCERS.exclusive_access();
    if rid > producers.len() - 1 {
        // Not long enough. Expand the Vec's capacity.
        let expand = rid - producers.len() + 1;
        for _ in 0 .. expand {
            producers.push(None);
        }
    }
}

/// Set a resource producer in RESOURCE_PRODUCERS.
///
/// Len of RESOURCE_PRODUCERS will be automatically
/// adjusted if it is not long enough.
fn set_resource_producer(
    rid: usize,
    resource_producer: Option<Weak<ResourceProducerHandle>>
) {
    adjust_resource_producers_len(rid);
    let mut producers = RESOURCE_PRODUCERS.exclusive_access();
    producers[rid] = resource_producer;
}

/// Detect whether resource of the given rid exists
/// (neither unallocated nor disposed).
fn resource_producer_exists(rid: usize) -> bool {
    let producers = RESOURCE_PRODUCERS.exclusive_access();
    let producer = producers.get(rid);
    if producer.is_none() {
        return false;
    }
    let producer = producer.unwrap();
    if producer.is_none() {
        return false;
    }
    if Weak::clone(producer.as_ref().unwrap()).upgrade().is_none() {
        return false;
    }
    true
}

/// Adjust len of AVAILABLE if it is not long enough.
fn adjust_available_vec_len(rid: usize) {
    let mut available = AVAILABLE.exclusive_access();
    if rid > available.len() - 1 {
        // Not long enough. Expand its capacity.
        let expand = rid - available.len() + 1;
        for _ in 0 .. expand {
            *available = available.push(0);
        }
    }
}

/// Adjust size of ALLOCATION if it is not large enough.
fn adjust_allocation_mat_size(tid: usize, rid: usize) {
    let mut allocation = ALLOCATION.exclusive_access();
    let rows = allocation.row_iter().len();
    let cols = allocation.column_iter().len();
    if tid > rows - 1 || rid > cols - 1 {
        // Not large enough. Expand its capacity.
        let new_rows = tid + 1;
        let new_cols = rid + 1;
        *allocation = allocation.clone().resize(new_rows, new_cols, 0);
    }
}

/// Adjust size of NEED if it is not large enough.
fn adjust_need_mat_size(tid: usize, rid: usize) {
    let mut need = NEED.exclusive_access();
    let rows = need.row_iter().len();
    let cols = need.column_iter().len();
    if tid > rows - 1 || rid > cols - 1 {
        // Not large enough. Expand its capacity.
        let new_rows = tid + 1;
        let new_cols = rid + 1;
        *need = need.clone().resize(new_rows, new_cols, 0);
    }
}

/// Submit need for the given thread before deadlock calculating.
fn submit_need(tid: usize, rid: usize, amount: usize) {
    adjust_need_mat_size(tid, rid);
    let need = NEED.exclusive_access();
    need[(tid, rid)] += amount;
    drop(need);
}

/// Remove need for the given thread after deadlock calculating.
fn remove_need(tid: usize, rid: usize, amount: usize) {
    adjust_need_mat_size(tid, rid);
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