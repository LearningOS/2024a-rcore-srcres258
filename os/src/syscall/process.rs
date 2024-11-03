//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::config::MAX_SYSCALL_NUM;
use crate::config::PAGE_SIZE;
use crate::fs::OpenFlags;
use crate::fs::open_file;
use crate::mm::MapPermission;
use crate::mm::VirtAddr;
use crate::mm::VirtPageNum;
use crate::mm::VPNRange;
use crate::mm::translated_refmut;
use crate::mm::translated_str;
use crate::mm::copy_data_to_current_user;
use crate::task::TaskStatus;
use crate::task::add_task;
use crate::task::current_task;
use crate::task::current_user_token;
use crate::task::exit_current_and_run_next;
use crate::task::suspend_current_and_run_next;
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

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time",
        current_task().unwrap().pid.0
    );

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
    trace!(
        "kernel:pid[{}] sys_task_info",
        current_task().unwrap().pid.0
    );

    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    let status = inner.task_status;
    let syscall_times = inner.task_info.syscall_times;
    let start_time = inner.task_info.start_time;
    drop(inner);
    drop(task);

    let current_time = get_time_us();
    let delta_time = start_time
        .map(|start| current_time - start)
        .unwrap_or(0); // task has not been started yet
    // Note that the unit of `delta_time` is microseconds.
    // We should convert it to milliseconds.
    let delta_time_ms = delta_time / 1000;
    // Create our version of TaskInfo on the kernel stack.
    let ti_kernel = TaskInfo {
        status,
        syscall_times,
        time: delta_time_ms
    };
    // Then copy it to the user space.
    copy_data_to_current_user(ti, &ti_kernel);

    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap",
        current_task().unwrap().pid.0
    );

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

    // Convert len in bytes to len in memory pages.
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
    let mut map_perm = MapPermission::U;
    if mem_r {
        map_perm |= MapPermission::R;
    }
    if mem_w {
        map_perm |= MapPermission::W;
    }
    if mem_x {
        map_perm |= MapPermission::X;
    }
    // Check whether the given virtual address has been already recorded to be mapped.
    let mut exist_record = false;
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    for (s, _l) in inner.mmap_records.iter() {
        if *s == vpn_start {
            exist_record = true;
            break;
        }
    }
    drop(inner);
    drop(task);
    if exist_record {
        return -1;
    }
    // Check whether there are some virtual addresses which have already been
    // mapped in the memory set of the current task within the mem_area range.
    let mut exist_mapped = false;
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    for vpn in VPNRange::new(vpn_start, vpn_end) {
        if inner.memory_set.is_mapped(vpn) {
            exist_mapped = true;
            break;
        }
    }
    drop(inner);
    drop(task);
    if exist_mapped {
        return -1;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // Map the virtual memory section in the memory set of the current task.
    inner.memory_set.insert_framed_area(va_start, va_end, map_perm);
    // Record this mmap operation.
    inner.mmap_records.push((vpn_start, len));
    drop(inner);
    drop(task);

    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );

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
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    for (i, (s, l)) in inner.mmap_records.iter().enumerate() {
        if *s == vpn_start && *l == len {
            idx = Some(i);
            break;
        }
    }
    drop(inner);
    drop(task);
    if idx.is_none() {
        return -1;
    }
    
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // Unmap the virtual memory section in the memory set of the current task.
    inner.memory_set.unmap_framed_area(va_start);
    // Remove the mmap operation record.
    inner.mmap_records.remove(idx.unwrap());
    drop(inner);
    drop(task);
    
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    
    let token = current_user_token();
    let path = translated_str(token, path);
    
    if let Some(app_inode) = open_file(&path, OpenFlags::RDONLY) {
        let data = app_inode.read_all();
        let current_task = current_task().unwrap();
        let child_task = current_task.spawn(data.as_slice());
        let child_pid = child_task.pid.0;
        // add new task to scheduler
        add_task(child_task);
        child_pid as isize
    } else {
        // The given path does not exist.
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );

    // Check the validity of the arguments at first.
    // Argument: prio
    // Requirement: Bigger than or equal to 2.
    if prio < 2 {
        return -1;
    }
    
    let prio_ = prio as u64;
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.task_info.priority = prio_;
    drop(inner);
    drop(task);
    
    prio
}
