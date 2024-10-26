//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        exit_current_and_run_next,
        suspend_current_and_run_next,
        op_on_current_task,
        TaskStatus
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");

    let mut status = None;
    let mut syscall_times = [0; MAX_SYSCALL_NUM];
    let mut start_time = None;

    op_on_current_task(|block| {
        status = Some(block.task_status);
        for (i, times) in block.task_info.syscall_times.iter().enumerate() {
            syscall_times[i] = *times;
        }
        start_time = block.task_info.start_time;
    });

    let current_time = get_time_us();
    let delta_time = match start_time {
        Some(start) => current_time - start,
        None => 0, // task has not been started yet
    };
    // Note that the unit of `delta_time` is microseconds.
    // We should convert it to milliseconds.
    let delta_time_ms = delta_time / 1_000;
    unsafe {
        (*ti).status = status.unwrap();
        (*ti).syscall_times = syscall_times;
        (*ti).time = delta_time_ms;
    }

    0
}
