//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::PAGE_SIZE,
    fs::{open_file, OpenFlags},
    mm::{
        translated_refmut, translated_str, MapPermission, VirtAddr, VirtPageNum, VPNRange,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskControlBlock, BIG_STRIDE,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
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
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0;
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

/// 阻塞版 waitpid
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    loop {
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();

        if !inner
            .children
            .iter()
            .any(|p| pid == -1 || pid as usize == p.getpid())
        {
            return -1;
        }

        let pair = inner.children.iter().enumerate().find(|(_, p)| {
            p.inner_exclusive_access().is_zombie()
                && (pid == -1 || pid as usize == p.getpid())
        });

        if let Some((idx, _)) = pair {
            let child = inner.children.remove(idx);
            assert_eq!(Arc::strong_count(&child), 1);
            let found_pid = child.getpid();
            let exit_code = child.inner_exclusive_access().exit_code;
            *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
            return found_pid as isize;
        } else {
            drop(inner);
            suspend_current_and_run_next();
        }
    }
}

pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel:pid[{}] sys_get_time", current_task().unwrap().pid.0);
    let token = current_user_token();
    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let dst = translated_refmut(token, ts);
    *dst = time_val;
    0
}

pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel:pid[{}] sys_mmap", current_task().unwrap().pid.0);

    if prot & !0x7 != 0 {
        return -1;
    }
    if prot & 0x7 == 0 {
        return -1;
    }
    if start & (PAGE_SIZE - 1) != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }

    let start_vpn: VirtPageNum = VirtAddr::from(start).into();
    let end_vpn: VirtPageNum = VirtAddr::from(start + len).ceil();

    let mut map_perm = MapPermission::U;
    if prot & 0x1 != 0 {
        map_perm |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        map_perm |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        map_perm |= MapPermission::X;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();

    for area in &inner.memory_set.areas {
        let a_start = area.vpn_range.get_start();
        let a_end = area.vpn_range.get_end();
        if a_end > start_vpn && end_vpn > a_start {
            return -1;
        }
    }

    inner.memory_set.insert_framed_area(
        VirtAddr::from(start),
        VirtAddr::from(start + len),
        map_perm,
    );

    0
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_munmap", current_task().unwrap().pid.0);

    if start & (PAGE_SIZE - 1) != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }

    let start_vpn: VirtPageNum = VirtAddr::from(start).into();
    let end_vpn: VirtPageNum = VirtAddr::from(start + len).ceil();

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let memory_set = &mut inner.memory_set;

    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if memory_set.page_table.translate(vpn).is_none() {
            return -1;
        }
    }

    let mut remove_idx = None;
    for (idx, area) in memory_set.areas.iter().enumerate() {
        if area.vpn_range.get_start() == start_vpn && area.vpn_range.get_end() == end_vpn {
            remove_idx = Some(idx);
            break;
        }
    }

    let Some(idx) = remove_idx else {
        return -1;
    };

    let mut area = memory_set.areas.remove(idx);
    area.unmap(&mut memory_set.page_table);

    0
}

pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// spawn：从文件系统按名字读 ELF，新建进程（不复制父进程地址空间）
pub fn sys_spawn(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_spawn", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let new_task = Arc::new(TaskControlBlock::new(all_data.as_slice()));
        let new_pid = new_task.pid.0;
        let current = current_task().unwrap();
        new_task.inner_exclusive_access().parent = Some(Arc::downgrade(&current));
        current
            .inner_exclusive_access()
            .children
            .push(new_task.clone());
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

pub fn sys_set_priority(prio: isize) -> isize {
    trace!("kernel:pid[{}] sys_set_priority", current_task().unwrap().pid.0);
    if prio < 2 {
        return -1;
    }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.priority = prio as usize;
    inner.pass = BIG_STRIDE / prio as usize;
    prio
}
