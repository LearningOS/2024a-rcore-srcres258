//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        change_program_brk,
        exit_current_and_run_next,
        suspend_current_and_run_next,
        op_on_current_task,
        TaskStatus,
    },
    mm::copy_data_to_current_user
};
use crate::timer::get_time_us;

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
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

    // Create our version of TimeVal on the kernel stack.
    let us = get_time_us();
    let ts_kernel = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };

    // Then copy the TimeVal data from the kernel space to the user space.
    copy_data_to_current_user(ts, &ts_kernel);

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");

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
    let delta_time_ms = delta_time / 1000;
    // Create our version of TaskInfo on the kernel stack.
    let ti_kernel = TaskInfo {
        status: status.unwrap(),
        syscall_times,
        time: delta_time_ms
    };
    // Then copy it to the user space.
    copy_data_to_current_user(ti, &ti_kernel);

    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    -1
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
