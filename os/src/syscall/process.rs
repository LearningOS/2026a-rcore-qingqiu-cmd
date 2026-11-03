//! Process management syscalls
use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next};
use crate::mm::translated_refmut;
use crate::task::{current_user_token,TASK_MANAGER};
use crate::timer::get_time_us;
use crate::mm::{PageTable,VirtAddr,MapPermission,VirtPageNum,VPNRange,PTEFlags};
use crate::config::PAGE_SIZE;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
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

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        // 0: 读用户地址 id 处的 1 字节
        0 => {
            let token = current_user_token();
            let page_table = PageTable::from_token(token);
            let va = VirtAddr::from(id);
            let vpn = va.floor();
            match page_table.translate(vpn) {
                Some(pte) => {
                    let flags = pte.flags();
                    if flags.contains(PTEFlags::R) && flags.contains(PTEFlags::U) {
                        let offset = va.page_offset();
                        pte.ppn().get_bytes_array()[offset] as isize
                    } else {
                        -1
                    }
                }
                None => -1,
            }
        }
        // 1: 写 data 到用户地址 id 处
        1 => {
            let token = current_user_token();
            let page_table = PageTable::from_token(token);
            let va = VirtAddr::from(id);
            let vpn = va.floor();
            match page_table.translate(vpn) {
                Some(pte) => {
                    let flags = pte.flags();
                    if flags.contains(PTEFlags::W) && flags.contains(PTEFlags::U) {
                        let offset = va.page_offset();
                        pte.ppn().get_bytes_array()[offset] = data as u8;
                        0
                    } else {
                        -1
                    }
                }
                None => -1,
            }
        }
        // 2: 查系统调用计数
        2 => 
            crate::task::get_syscall_count(id),
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if port & !0x7 != 0 {
	    return -1;
	}
    if port & 0x7 == 0 {
	    return -1;
	}
    if start & (PAGE_SIZE - 1) != 0 {
	    return -1;
	}
    if len == 0 {
	    return 0;
	}
    let start_vpn: VirtPageNum = VirtAddr::from(start).into();
    let end_vpn: VirtPageNum = VirtAddr::from(start+len).ceil();
    let mut map_perm = MapPermission::U;
    if port & 0x1 != 0 {map_perm |= MapPermission::R;}
    if port & 0x2 != 0 {map_perm |= MapPermission::W;}
    if port & 0x4 != 0 {map_perm |= MapPermission::X;}

    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let cur = inner.current_task;

    for area in &inner.tasks[cur].memory_set.areas {
		let a_start = area.vpn_range.get_start();
		let a_end = area.vpn_range.get_end();
		if a_end > start_vpn && end_vpn > a_start {
			return -1;
		}
	}
	inner.tasks[cur].memory_set.insert_framed_area(
		VirtAddr::from(start),
		VirtAddr::from(start+len),
		map_perm,
	);
	0
}


// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start & (PAGE_SIZE - 1) != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }
    let start_vpn: VirtPageNum = VirtAddr::from(start).into();
    let end_vpn: VirtPageNum = VirtAddr::from(start + len).ceil();
    
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let cur = inner.current_task;
    let memory_set = &mut inner.tasks[cur].memory_set;

    for vpn in VPNRange::new(start_vpn,end_vpn){
	if memory_set.page_table.translate(vpn).is_none(){
			return -1;
			}
		}
    let mut remove_idx = None;
    for (idx,area) in memory_set.areas.iter().enumerate(){
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
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
