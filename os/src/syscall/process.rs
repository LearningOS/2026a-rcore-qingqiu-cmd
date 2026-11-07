//! Process management syscalls
use alloc::sync::Arc;
use crate::task::BIG_STRIDE;
use crate::{
    config::PAGE_SIZE,
    loader::get_app_data_by_name,
    mm::{
        translated_refmut, translated_str, MapPermission, VirtAddr, VirtPageNum,
        VPNRange,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
	TaskControlBlock,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
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
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    loop {
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();

        // 没有匹配的子进程 → 返回 -1
        if !inner
            .children
            .iter()
            .any(|p| pid == -1 || pid as usize == p.getpid())
        {
            return -1;
        }

        // 找僵尸态且 pid 匹配的子进程
        let pair = inner.children.iter().enumerate().find(|(_, p)| {
            p.inner_exclusive_access().is_zombie()
                && (pid == -1 || pid as usize == p.getpid())
        });

        if let Some((idx, _)) = pair {
            // 找到了，回收子进程
            let child = inner.children.remove(idx);
            assert_eq!(Arc::strong_count(&child), 1);
            let found_pid = child.getpid();
            let exit_code = child.inner_exclusive_access().exit_code;
            *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
            return found_pid as isize;
        } else {
            // 还没退出，让出 CPU 等下次
            drop(inner);
            suspend_current_and_run_next();
        }
    }
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
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

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap",
        current_task().unwrap().pid.0
    );
    if prot & !0x7 != 0{
		return -1;
	}
    if prot & 0x7 == 0{
		return -1;
	}
    if start & (PAGE_SIZE-1) != 0{
		return -1;
	}
    if len == 0{
		return 0;
	}

	let start_vpn:VirtPageNum = VirtAddr::from(start).into();
	let end_vpn:VirtPageNum = VirtAddr::from(start+len).ceil();

	let mut map_perm = MapPermission::U;
	if prot & 0x1 != 0{map_perm |= MapPermission::R;}
	if prot & 0x2 != 0{map_perm |= MapPermission::W;}
	if prot & 0x4 != 0{map_perm |= MapPermission::X;}

	let task = current_task().unwrap();
	let mut inner = task.inner_exclusive_access();

	for area in &inner.memory_set.areas{
		let a_start = area.vpn_range.get_start();
		let a_end = area.vpn_range.get_end();
		if a_end>start_vpn && end_vpn > a_start{return -1;}
	}
	inner.memory_set.insert_framed_area(
		VirtAddr::from(start),
		VirtAddr::from(start+len),
		map_perm,
		);
	0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );
	if start & (PAGE_SIZE-1) != 0{return -1;}
	if len == 0{return 0;}
	
	let start_vpn:VirtPageNum = VirtAddr::from(start).into();
	let end_vpn:VirtPageNum = VirtAddr::from(start+len).ceil();

	let task = current_task().unwrap();
	let mut inner = task.inner_exclusive_access();
	let memory_set = &mut inner.memory_set;

	for vpn in VPNRange::new(start_vpn,end_vpn){
		if memory_set.page_table.translate(vpn).is_none(){return -1;}
	}
	let mut remove_idx = None;
	for (idx,area) in memory_set.areas.iter().enumerate(){
		if area.vpn_range.get_start() == start_vpn && area.vpn_range.get_end() == end_vpn {
				remove_idx = Some(idx);
				break;
				}
			}
	let Some(idx) = remove_idx else {return -1;};
	let mut area = memory_set.areas.remove(idx);
	area.unmap(&mut memory_set.page_table);
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
    trace!("kernel:pid[{}] sys_spawn", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path_str = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path_str.as_str()) {
        let new_task = Arc::new(TaskControlBlock::new(data));
        let new_pid = new_task.pid.0;
        let current = current_task().unwrap();

        //子进程记录父进程（Weak 避免循环引用）
        {
            let mut new_inner = new_task.inner_exclusive_access();
            new_inner.parent = Some(Arc::downgrade(&current));
        }

        //父进程的 children 里加子进程
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

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );
	if prio < 2 {return -1;}
	let task = current_task().unwrap();
	let mut inner = task.inner_exclusive_access();
	inner.priority = prio as usize;
	inner.pass = BIG_STRIDE / prio as usize;
	prio
}
