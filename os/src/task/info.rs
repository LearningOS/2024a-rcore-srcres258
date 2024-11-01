use core::default::Default;

use crate::config::MAX_SYSCALL_NUM;

/// The recorded information of a task.
/// Used for providing results for task information querying syscalls.
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

impl TaskInfo {
    /// Create a new empty task context
    pub fn zero_init() -> Self {
        Self {
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: 0,
        }
    }
}

impl Default for TaskInfo {
    fn default() -> Self {
        Self::zero_init()
    }
}