use alloc::vec::Vec;

/// Process resource usage recorder. Used to detect deadlock for threads under this process.
pub struct ProcessResource {
    /// Available vector
    pub available: Vec<usize>,
    /// Allocation matrix
    pub allocation: Vec<Vec<usize>>,
    /// Need matrix
    pub need: Vec<Vec<usize>>
}

impl ProcessResource {
    /// Initialise a new process resource.
    ///
    /// Vectors' size is 20 and matrices' size is 20x20.
    pub fn new() -> Self {
        let mut inner_vec = Vec::new();
        for _ in 0 .. 20 {
            inner_vec.push(0);
        }

        let available = inner_vec.clone();

        let mut allocation = Vec::new();
        for _ in 0 .. 20 {
            allocation.push(inner_vec.clone());
        }

        let mut need = Vec::new();
        for _ in 0 .. 20 {
            need.push(inner_vec.clone());
        }

        Self { available, allocation, need }
    }

    /// Detect deadlock from the current process resource.
    ///
    /// If deadlock exists at present, true is returned.
    pub fn detect_deadlock(&self) -> bool {
        let thread_count = self.allocation.len();
        let resource_count = self.available.len();

        let mut work = self.available.clone();
        let mut finish = Vec::new();
        for _ in 0 .. thread_count {
            finish.push(false);
        }

        // Walk through the threads.
        loop {
            let mut should_leave_loop = true;
            for i in 0 .. thread_count {
                if !finish[i] {
                    let mut able_to_finish = true;
                    for j in 0 .. resource_count {
                        able_to_finish = able_to_finish && (self.need[i][j] <= work[j]);
                    }
                    if able_to_finish {
                        should_leave_loop = false;
                        for j in 0 .. resource_count {
                            work[j] += self.allocation[i][j];
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
        
        !no_deadlock
    }
}