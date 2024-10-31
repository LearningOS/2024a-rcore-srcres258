//! Process management syscalls
use crate::{
    config::{
        MAX_SYSCALL_NUM,
        PAGE_SIZE
    },
    task::{
        change_program_brk,
        exit_current_and_run_next,
        suspend_current_and_run_next,
        op_on_current_task,
        TaskStatus,
    },
    mm::{
        copy_data_to_current_user,
        VirtAddr,
        VirtPageNum,
        MapPermission,
        VPNRange
    }
};
use crate::task::op_on_current_task_mut;
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
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    info!("sys_mmap: start = {}, len = {}, port = {}", start, len, port);

    // Check the validity of the arguments at first.
    // Argument: start
    // Requirement: Aligned by page size.
    let va_start = VirtAddr(start);
    if !va_start.aligned() {
        return -1;
    }
    // Argument: port
    // Requirement: At least one of the 0th, 1st or 2nd digit is set to 1,
    // and the other digits must be 0.
    if port & !0x7 != 0 {
        return -1;
    }
    if port & 0x7 == 0 {
        return -1;
    }

    // Convert len in bytes to len in memory pages
    let mut len_page = len / PAGE_SIZE;
    // If there are tail bytes that do not meet up with one page size,
    // put them into one memory page directly.
    if len % PAGE_SIZE > 0 {
        len_page += 1;
    } else if len == 0 {
        len_page = 1;
    }
    // Get the memory permission (R/W/X).
    let mem_r = port & 1 == 1;
    let mem_w = (port >> 1) & 1 == 1;
    let mem_x = (port >> 2) & 1 == 1;
    // Calculate the ending virtual address.
    let vpn_start = VirtPageNum::from(va_start);
    let mut vpn_end = vpn_start;
    vpn_end.0 += len_page;
    let va_end = VirtAddr::from(vpn_end);
    // Construct information for the virtual memory section being mapped.
    let mut map_perm = MapPermission::U; // accessible from user level
    if mem_r {
        map_perm |= MapPermission::R;
    }
    if mem_w {
        map_perm |= MapPermission::W;
    }
    if mem_x {
        map_perm |= MapPermission::X;
    }
    info!("vpn_start = {}, vpn_end = {}, mem_r = {}, mem_w = {}, mem_x = {}", vpn_start.0, vpn_end.0, mem_r, mem_w, mem_x);
    // Check whether the given virtual address has been already recorded to be mapped.
    let mut exist_record = false;
    op_on_current_task(|block| {
        for (s, _l) in block.mmap_records.iter() {
            if *s == vpn_start {
                exist_record = true;
                break;
            }
        }
    });
    if exist_record {
        return -1;
    }
    // Check whether there are some virtual addresses which have already been
    // mapped in the memory set of the current task within the mem_area range.
    let mut exist_mapped = false;
    op_on_current_task(|block| {
        for vpn in VPNRange::new(vpn_start, vpn_end) {
            if block.memory_set.is_mapped(vpn) {
                info!("page {} is mapped!", vpn.0);
                exist_mapped = true;
                break;
            }
            info!("page {} is not mapped!", vpn.0);
        }
    });
    if exist_mapped {
        return -1;
    }

    info!("va_start = {}, va_end = {}, map_perm = {:?}", va_start.0, va_end.0, map_perm);
    op_on_current_task_mut(|block| {
        // Map the virtual memory section in the memory set of the current task.
        block.memory_set.insert_framed_area(va_start, va_end, map_perm);
        // Record this mmap operation.
        block.mmap_records.push((vpn_start, len));
    });

    info!("sys_mmap finished!");
    
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    // Check the validity of the arguments at first.
    // Argument: start
    // Requirement: Aligned by page size.
    let va_start = VirtAddr(start);
    if !va_start.aligned() {
        return -1;
    }

    // Check whether there are a matching mmap record for the current task,
    // and get the matching record.
    let vpn_start = VirtPageNum::from(va_start);
    let mut idx = None;
    op_on_current_task(|block| {
        for (i, (s, l)) in block.mmap_records.iter().enumerate() {
            if *s == vpn_start && *l == len {
                idx = Some(i);
                break;
            }
        }
    });
    if idx.is_none() {
        return -1;
    }

    op_on_current_task_mut(|block| {
        // Unmap the virtual memory section in the memory set of the current task.
        block.memory_set.unmap_framed_area(va_start);
        // Remove the mmap operation record.
        block.mmap_records.remove(idx.unwrap());
    });

    info!("sys_munmap finished!");

    0
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
