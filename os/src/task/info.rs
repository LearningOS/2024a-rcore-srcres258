use core::default::Default;

use crate::config::MAX_SYSCALL_NUM;

/// The recorded information of a task.
/// Used for providing results for task information querying syscalls.
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// The numbers of syscall called by task.
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Start time of the task (unit: us).
    /// None if the task has not been started yet.
    pub start_time: Option<usize>,
}

impl TaskInfo {
    /// Create a new empty task information.
    pub fn zero_init() -> Self {
        Self {
            syscall_times: [0; MAX_SYSCALL_NUM],
            start_time: None,
        }
    }
}

impl Default for TaskInfo {
    fn default() -> Self {
        Self::zero_init()
    }
}