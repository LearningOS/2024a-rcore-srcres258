use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use lazy_static::lazy_static;
use crate::debug;
use crate::sync::UPSafeCell;
use crate::task::RecycleAllocator;

lazy_static! {
    static ref AVAILABLE: UPSafeCell<Vec<usize>> = unsafe {
        UPSafeCell::new({
            let mut result = Vec::new();
            for _ in 0 .. 100 {
                result.push(0);
            }
            result
        })
    };
    static ref ALLOCATION: UPSafeCell<Vec<Vec<usize>>> = unsafe {
        UPSafeCell::new({
            let mut inner = Vec::new();
            for _ in 0 .. 100 {
                inner.push(0);
            }
            let mut result = Vec::new();
            for _ in 0 .. 100 {
                result.push(inner.clone());
            }
            result
        })
    };
    static ref NEED: UPSafeCell<Vec<Vec<usize>>> = unsafe {
        UPSafeCell::new({
            let mut inner = Vec::new();
            for _ in 0 .. 100 {
                inner.push(0);
            }
            let mut result = Vec::new();
            for _ in 0 .. 100 {
                result.push(inner.clone());
            }
            result
        })
    };

    static ref RID_ALLOCATOR: UPSafeCell<RecycleAllocator> = unsafe {
        UPSafeCell::new(RecycleAllocator::new())
    };
    static ref RESOURCE_PRODUCERS: UPSafeCell<Vec<Option<Weak<ResourceProducerHandle>>>> = unsafe {
        UPSafeCell::new({
            let mut result = Vec::new();
            result.push(None);
            result
        })
    };
}

/// Resource producer handles. This stands for a type of
/// resources provided by some entity.
pub struct ResourceProducerHandle {
    // immutable part
    /// Resource ID of this type of resource.
    rid: usize,
    /// Resource initial amount of this type of resource.
    /// This should be immutable after initialisation.
    initial_amount: usize,

    // mutable part
    inner: UPSafeCell<ResourceProducerHandleInner>
}

pub struct ResourceProducerHandleInner {
    /// Resource amount that this type of resource remains.
    amount: usize
}

/// Resource consumer handle. This stands for a consumer
/// that holds some amount of resources.
pub struct ResourceConsumerHandle {
    /// Thread ID which the resource is possessed by.
    tid: usize,
    /// Resource ID that the thread possesses.
    rid: usize,
    /// Resource amount that the thread possesses.
    amount: usize
}

pub struct ResourceNeedHandle {
    tid: usize,
    rid: usize,
    amount: usize
}

/// A collection of resource consumer handles in order to
/// manage them more conveniently.
pub struct ResourceConsumerHandleCollection {
    /// Thread ID which the resources within this collection are possessed by.
    tid: usize,
    /// Consumer resource handles possessed by this collection.
    handles: Vec<ResourceConsumerHandle>,
    /// Need handles possessed by this collection.
    needs: Vec<ResourceNeedHandle>
}

/// Operation result enum for syncing or locking operations
/// (acquisition of resources).
pub enum OperationResult {
    /// The operation was done without faults.
    Done,
    /// Deadlock is detected and the operation is denied.
    DeadlockDetected
}

impl ResourceProducerHandle {
    /// Create a new resource producer handle with the given initial amount.
    pub fn new(initial_amount: usize) -> Arc<Self> {
        // Allocate a rid for this type of resource.
        let mut rid_allocator = RID_ALLOCATOR.exclusive_access();
        let rid = rid_allocator.alloc();
        drop(rid_allocator);
        // Set value of self in AVAILABLE vector to initial amount.
        adjust_available_vec_len(rid);
        let mut available = AVAILABLE.exclusive_access();
        available[rid] = initial_amount;
        drop(available);
        // Construct self.
        let inner = unsafe {
            UPSafeCell::new(ResourceProducerHandleInner {
                amount: 0
            })
        };
        let result = Arc::new(Self {
            rid,
            initial_amount,
            inner
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
    fn allocate(&self, amount: usize) {
        let rid = self.rid;

        info!("ResourceProducerHandle: allocating rid[{}] with amount[{}]",
            rid, amount);

        adjust_available_vec_len(rid);
        let mut available = AVAILABLE.exclusive_access();
        available[rid] -= amount;

        let mut inner = self.inner.exclusive_access();
        inner.amount -= amount;

        info!("After allocation: {}", available[rid]);
    }

    /// Deallocate amount of this type of resource.
    ///
    /// This method should only be called by ResourceProducerHandle
    /// during its resource allocation process.
    fn deallocate(&self, amount: usize) {
        let rid = self.rid;

        info!("ResourceProducerHandle: deallocating rid[{}] with amount[{}]",
            rid, amount);

        adjust_available_vec_len(rid);
        let mut available = AVAILABLE.exclusive_access();
        available[rid] += amount;

        let mut inner = self.inner.exclusive_access();
        inner.amount += amount;
    }

    /// Get resource ID of these type of resources.
    #[allow(unused)]
    pub fn rid(&self) -> usize {
        self.rid
    }

    /// Get the initial amount of these type of resources.
    #[allow(unused)]
    pub fn initial_amount(self) -> usize {
        self.initial_amount
    }

    /// Get the resource amount remaining.
    #[allow(unused)]
    pub fn amount(&self) -> usize {
        self.inner.exclusive_access().amount
    }
}

impl Drop for ResourceProducerHandle {
    fn drop(&mut self) {
        // Set value of self in AVAILABLE vector to zero.
        adjust_available_vec_len(self.rid);
        let mut available = AVAILABLE.exclusive_access();
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
        // Adjust scale of AVAILABLE, ALLOCATION, NEED at first
        // to ensure their capacity.
        adjust_available_vec_len(rid);
        adjust_allocation_mat_size(tid, rid);
        adjust_need_mat_size(tid, rid);

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
        let producer = producers.get(rid).unwrap()
            .as_ref().unwrap().upgrade().unwrap();
        producer.allocate(amount);
        drop(producer);
        drop(producers);

        // Mark the given amount of resource is allocated for this thread.
        adjust_allocation_mat_size(tid, rid);
        let mut allocation = ALLOCATION.exclusive_access();
        allocation[tid][rid] += amount;
        drop(allocation);

        // Construct self and return.
        Some(Self { tid, rid, amount })
    }

    /// Submit need of the given amount for deadlock calculating.
    pub fn submit_need(&self, amount: usize) {
        submit_need(self.tid, self.rid, amount);
    }

    /// Remove need of the given amount for deadlock calculating.
    pub fn remove_need(&self, amount: usize) {
        remove_need(self.tid, self.rid, amount);
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
        let producer = producers.get(rid).unwrap()
            .as_ref().unwrap().upgrade().unwrap();
        producer.allocate(amount);
        drop(producer);
        drop(producers);

        // Mark the given amount of resource is allocated for this thread.
        adjust_allocation_mat_size(tid, rid);
        let mut allocation = ALLOCATION.exclusive_access();
        allocation[tid][rid] += amount;
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
            let mut allocation = ALLOCATION.exclusive_access();
            allocation[tid][rid] = 0;
        } else {
            // Call into ResourceProducerHandle for resource deallocation marking.
            let producers = RESOURCE_PRODUCERS.exclusive_access();
            let producer = producers.get(rid).unwrap()
                .as_ref().unwrap().upgrade().unwrap();
            producer.deallocate(amount);
            drop(producer);
            drop(producers);

            // Mark the given amount of resource is deallocated for this thread.
            adjust_allocation_mat_size(tid, rid);
            let mut allocation = ALLOCATION.exclusive_access();
            allocation[tid][rid] -= amount;
            drop(allocation);
        }

        // Record this deallocation.
        self.amount -= amount;
    }

    /// Get the thread ID whose thread holds this handle.
    #[allow(unused)]
    pub fn tid(&self) -> usize {
        self.tid
    }

    /// Get the resource ID that the handle possesses.
    #[allow(unused)]
    pub fn rid(&self) -> usize {
        self.rid
    }

    /// Get the resource amount that the handle possesses.
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
    /// Create a new collection with a thread ID that
    /// this collection belongs to.
    pub fn new(tid: usize) -> ResourceConsumerHandleCollection {
        Self {
            tid,
            handles: Vec::new(),
            needs: Vec::new()
        }
    }

    /// Submit need of the given amount for deadlock calculating.
    pub fn submit_need(&mut self, rid: usize, amount: usize) {
        let need = self.needs.iter_mut().find(|n| n.rid == rid);
        match need {
            Some(n) => {
                n.submit(amount);
            }
            None => {
                let mut n = ResourceNeedHandle::new(self.tid, rid);
                n.submit(amount);
                self.needs.push(n);
            }
        }
    }

    /// Remove need of the given amount for deadlock calculating.
    pub fn remove_need(&mut self, rid: usize, amount: usize) {
        let need = self.needs.iter_mut().find(|n| n.rid == rid);
        if let Some(n) = need {
            n.remove(amount);
        }
    }

    /// Attempt to allocate amount of the given resource within this collection.
    ///
    /// If failed to allocate (deadlock is detected), false is returned.
    pub fn allocate(&mut self, rid: usize, amount: usize) -> bool {
        info!("tid[{}] is trying to allocate rid[{}] with amount[{}]",
            self.tid, rid, amount);

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

impl ResourceNeedHandle {
    pub fn new(tid: usize, rid: usize) -> ResourceNeedHandle {
        Self {
            tid,
            rid,
            amount: 0
        }
    }
    
    pub fn submit(&mut self, amount: usize) {
        submit_need(self.tid, self.rid, amount);
        self.amount += amount;
    }
    
    pub fn remove(&mut self, amount: usize) {
        remove_need(self.tid, self.rid, amount);
        self.amount -= amount;
    }
}

impl Drop for ResourceNeedHandle {
    fn drop(&mut self) {
        self.remove(self.amount);
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
    // info!("adjust_available_vec_len rid[{}]", rid);

    let mut available = AVAILABLE.exclusive_access();
    if rid > available.len() - 1 {
        // Not long enough. Expand its capacity.
        let expand = rid - available.len() + 1;
        for _ in 0 .. expand {
            available.push(0);
        }
    }

    // info!("After adjustment: {}", available.len());
}

/// Adjust size of ALLOCATION if it is not large enough.
fn adjust_allocation_mat_size(tid: usize, rid: usize) {
    // info!("adjust_allocation_mat_size tid[{}] rid[{}]", tid, rid);

    let mut allocation = ALLOCATION.exclusive_access();
    if allocation.is_empty() {
        allocation.push(Vec::new());
    }
    let rows = allocation.len();
    let cols = allocation[0].len();
    if tid > rows - 1 || rid > cols - 1 {
        // Not large enough. Expand its capacity.
        let new_rows = tid + 1;
        let new_cols = rid + 1;
        let delta_cols = new_cols - cols;
        for ri in 0 .. new_rows {
            let row = allocation.get_mut(ri);
            match row {
                Some(row) => {
                    for _ in 0 .. delta_cols {
                        row.push(0);
                    }
                }
                None => {
                    let mut new_col = Vec::new();
                    for _ in 0 .. new_cols {
                        new_col.push(0);
                    }
                    allocation.push(new_col);
                }
            }
        }
    }

    // info!("After adjustment: {}, {}", allocation.len(), allocation[0].len());
}

/// Adjust size of NEED if it is not large enough.
fn adjust_need_mat_size(tid: usize, rid: usize) {
    // info!("adjust_need_mat_size tid[{}] rid[{}]", tid, rid);

    let mut need = NEED.exclusive_access();
    if need.is_empty() {
        need.push(Vec::new());
    }
    let rows = need.len();
    let cols = need[0].len();
    if tid > rows - 1 || rid > cols - 1 {
        // Not large enough. Expand its capacity.
        let new_rows = tid + 1;
        let new_cols = rid + 1;
        let delta_cols = new_cols - cols;
        for ri in 0 .. new_rows {
            let row = need.get_mut(ri);
            match row {
                Some(row) => {
                    for _ in 0 .. delta_cols {
                        row.push(0);
                    }
                }
                None => {
                    let mut new_col = Vec::new();
                    for _ in 0 .. new_cols {
                        new_col.push(0);
                    }
                    need.push(new_col);
                }
            }
        }
    }

    // info!("After adjustment: {}, {}", need.len(), need[0].len());
}

/// Submit need for the given thread before deadlock calculating.
fn submit_need(tid: usize, rid: usize, amount: usize) {
    info!("submit_need tid[{}] rid[{}] amount[{}]", tid, rid, amount);
    adjust_need_mat_size(tid, rid);
    let mut need = NEED.exclusive_access();
    need[tid][rid] += amount;
    info!("submit_need: {}", need[tid][rid]);
    drop(need);
}

/// Remove need for the given thread after deadlock calculating.
fn remove_need(tid: usize, rid: usize, amount: usize) {
    info!("remove_need tid[{}] rid[{}] amount[{}]", tid, rid, amount);
    adjust_need_mat_size(tid, rid);
    let mut need = NEED.exclusive_access();
    need[tid][rid] -= amount;
    info!("remove_need: {}", need[tid][rid]);
    drop(need);
}

/// Detect whether deadlock might happen under the current circumstance.
///
/// If there is a deadlock, true is returned.
pub fn detect_deadlock() -> bool {
    info!("Beginning detect_deadlock");

    // Get thread count.
    let allocation = ALLOCATION.exclusive_access();
    let thread_count = allocation.len();
    // Get resource count.
    let available = AVAILABLE.exclusive_access();
    let resource_count = available.len();
    // Initialise Work and Finish Vec.
    let mut work: Vec<usize> = available.clone();
    let mut finish = Vec::new();
    for _ in 0 .. thread_count {
        finish.push(false);
    }

    // Here the col len of NEED and ALLOCATION might be smaller than
    // the len of ALLOCATION.
    // So resize their scale before the next operations to keep the
    // kernel from panicking.
    adjust_need_mat_size(thread_count - 1, resource_count - 1);
    drop(allocation);
    adjust_allocation_mat_size(thread_count - 1, resource_count - 1);
    let allocation = ALLOCATION.exclusive_access();

    // Walk through the threads.
    info!("thread_count={}, resource_count={}",
        thread_count, resource_count);
    let need = NEED.exclusive_access();
    println!("[AVAILABLE]");
    debug::print_vec(&available);
    println!("[ALLOCATION]");
    debug::print_vec_2d(&allocation);
    println!("[NEED]");
    debug::print_vec_2d(&need);
    loop {
        let mut should_leave_loop = true;
        for i in 0 .. thread_count {
            if !finish[i] {
                let mut able_to_finish = true;
                for j in 0 .. resource_count {
                    able_to_finish = able_to_finish && (need[i][j] <= work[j]);
                }
                if able_to_finish {
                    should_leave_loop = false;
                    for j in 0 .. resource_count {
                        work[j] += allocation[i][j];
                    }
                    finish[i] = true;
                }
            }
        }
        if should_leave_loop {
            break;
        }
    }

    // Sum up the results to discover whether there is a deadlock.
    let mut no_deadlock = true;
    for v in finish.iter() {
        no_deadlock = no_deadlock && *v;
    }

    info!("no_deadlock: {}", no_deadlock);

    !no_deadlock
}