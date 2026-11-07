use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

fn current_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

/// 银行家算法安全检查
fn is_safe(
    n: usize,
    m: usize,
    mut work: Vec<usize>,
    allocation: &Vec<Vec<usize>>,
    need: &Vec<Vec<usize>>,
) -> bool {
    let mut finish = vec![false; n];
    loop {
        let mut found = false;
        for i in 0..n {
            if !finish[i] {
                let mut ok = true;
                for j in 0..m {
                    if need[i][j] > work[j] {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    for j in 0..m {
                        work[j] += allocation[i][j];
                    }
                    finish[i] = true;
                    found = true;
                }
            }
        }
        if !found {
            break;
        }
    }
    finish.iter().all(|&x| x)
}

/// 信号量死锁检测（银行家算法）
fn banker_check_sem(sem_list: &[Option<Arc<Semaphore>>], req_idx: usize) -> bool {
    let mut sem_info = Vec::new();
    for s_opt in sem_list {
        let s = s_opt.as_ref().unwrap();
        let (alloc, wait_tids) = s.get_snapshot();
        let total = s.get_total_count();
        sem_info.push((total, alloc, wait_tids));
    }
    let m = sem_info.len();
    if m == 0 {
        return true;
    }

    let mut all_tids = Vec::new();
    for (_total, alloc, waits) in &sem_info {
        for &(tid, _cnt) in alloc {
            if !all_tids.contains(&tid) {
                all_tids.push(tid);
            }
        }
        for &tid in waits {
            if !all_tids.contains(&tid) {
                all_tids.push(tid);
            }
        }
    }
    let n = all_tids.len();
    if n == 0 {
        return true;
    }

    let mut allocation = vec![vec![0; m]; n];
    let mut need = vec![vec![0; m]; n];
    let mut available = Vec::with_capacity(m);

    for (j, &(total, ref alloc, ref waits)) in sem_info.iter().enumerate() {
        let sum_hold: usize = alloc.iter().map(|(_, c)| *c).sum();
        available.push(total.saturating_sub(sum_hold));
        for &(tid, cnt) in alloc {
            let ti = all_tids.iter().position(|&x| x == tid).unwrap();
            allocation[ti][j] = cnt;
        }
        for &tid in waits {
            let ti = all_tids.iter().position(|&x| x == tid).unwrap();
            need[ti][j] = 1;
        }
    }

    let tid = current_tid();
    let ti = match all_tids.iter().position(|&x| x == tid) {
        Some(v) => v,
        None => {
            all_tids.push(tid);
            allocation.push(vec![0; m]);
            need.push(vec![0; m]);
            all_tids.len() - 1
        }
    };

    info!(
        "[banker_sem] req_tid={} req_sem={} all_tids={:?} available={:?}",
        tid, req_idx, all_tids, available
    );
    for (i, t) in all_tids.iter().enumerate() {
        crate::println!("[banker_sem] tid={} allocation={:?} need={:?}", t, allocation[i], need[i]);
    }

    // 有可用资源，down 立刻成功，不阻塞，不检测
    if available[req_idx] > 0 {
        crate::println!("[banker_sem] req_sem={} result=true (available>0)", req_idx);
        return true;
    }
    // 无可用资源，申请会阻塞，模拟“进入等待”
    need[ti][req_idx] = 1;

    let result = is_safe(all_tids.len(), m, available.clone(), &allocation, &need);
    crate::println!("[banker_sem] req_sem={} result={}", req_idx, result);
    result
}

/// mutex 死锁检测（银行家算法）
fn banker_check_mutex(mutex_list: &[Option<Arc<dyn Mutex>>], req_idx: usize) -> bool {
    let m = mutex_list.len();
    if m == 0 {
        return true;
    }

    let mut all_tids = Vec::new();
    for m_opt in mutex_list {
        let m = m_opt.as_ref().unwrap();
        if let Some(owner) = m.get_owner() {
            if !all_tids.contains(&owner) {
                all_tids.push(owner);
            }
        }
    }
    let n = all_tids.len();

    let mut allocation = vec![vec![0; m]; n];
    let mut need = vec![vec![0; m]; n];
    let mut available = vec![1; m];

    for (j, m_opt) in mutex_list.iter().enumerate() {
        let m = m_opt.as_ref().unwrap();
        if let Some(owner) = m.get_owner() {
            available[j] = 0;
            if let Some(ti) = all_tids.iter().position(|&x| x == owner) {
                allocation[ti][j] = 1;
            }
        }
    }

    let tid = current_tid();
    let ti = match all_tids.iter().position(|&x| x == tid) {
        Some(v) => v,
        None => {
            all_tids.push(tid);
            allocation.push(vec![0; m]);
            need.push(vec![0; m]);
            all_tids.len() - 1
        }
    };

    info!(
        "[banker_mtx] req_tid={} req_mtx={} all_tids={:?} available={:?}",
        tid, req_idx, all_tids, available
    );

    if available[req_idx] == 1 {
        crate::println!("[banker_mtx] req_mtx={} result=true (free)", req_idx);
        return true;
    }
    need[ti][req_idx] = 1;

    for (j, m_opt) in mutex_list.iter().enumerate() {
        let m = m_opt.as_ref().unwrap();
        for (i, &t) in all_tids.iter().enumerate() {
            if m.is_waiting(t) {
                need[i][j] = 1;
            }
        }
    }

    let result = is_safe(all_tids.len(), m, available.clone(), &allocation, &need);
    crate::println!("[banker_mtx] req_mtx={} result={}", req_idx, result);
    result
}

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut inner = process.inner_exclusive_access();
    if let Some(id) = inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        inner.mutex_list[id] = mutex;
        id as isize
    } else {
        inner.mutex_list.push(mutex);
        inner.mutex_list.len() as isize - 1
    }
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    const DEAD_RET: isize = -0xDEAD;
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let de = inner.deadlock_enabled;
    let mtx = inner.mutex_list[mutex_id].as_ref().unwrap().clone();
    let mtx_list = inner.mutex_list.clone();
    drop(inner);

    if de {
        if mtx.get_owner() == Some(current_tid()) {
            crate::println!("[mtx_lock] tid={} mtx={} REENTRY deadlock", current_tid(), mutex_id);
            return DEAD_RET;
        }
        if mtx.get_owner().is_some() && !banker_check_mutex(&mtx_list, mutex_id) {
            crate::println!("[mtx_lock] tid={} mtx={} BANKER deadlock", current_tid(), mutex_id);
            return DEAD_RET;
        }
    }
    mtx.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let mtx = inner.mutex_list[mutex_id].as_ref().unwrap().clone();
    drop(inner);
    mtx.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let sem = Some(Arc::new(Semaphore::new(res_count)));
    if let Some(id) = inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        inner.semaphore_list[id] = sem;
        id as isize
    } else {
        inner.semaphore_list.push(sem);
        inner.semaphore_list.len() as isize - 1
    }
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    const DEAD_RET: isize = -0xDEAD;
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let de = inner.deadlock_enabled;
    let sem = inner.semaphore_list[sem_id].as_ref().unwrap().clone();
    let sem_list = inner.semaphore_list.clone();
    drop(inner);

    info!(
        "[sem_down] tid={} sem={} count={} de={}",
        current_tid(), sem_id, sem.get_count(), de
    );

    if de && !banker_check_sem(&sem_list, sem_id) {
        crate::println!("[sem_down] tid={} sem={} DEADLOCK", current_tid(), sem_id);
        return DEAD_RET;
    }
    sem.down();
    0
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let sem = inner.semaphore_list[sem_id].as_ref().unwrap().clone();
    drop(inner);

    info!(
        "[sem_up] tid={} sem={} count_before={}",
        current_tid(), sem_id, sem.get_count()
    );
    sem.up();
    0
}

pub fn sys_condvar_create() -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let cond = Some(Arc::new(Condvar::new()));
    if let Some(id) = inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        inner.condvar_list[id] = cond;
        id as isize
    } else {
        inner.condvar_list.push(cond);
        inner.condvar_list.len() as isize - 1
    }
}

pub fn sys_condvar_wait(cond_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let cond = inner.condvar_list[cond_id].as_ref().unwrap().clone();
    let mtx = inner.mutex_list[mutex_id].as_ref().unwrap().clone();
    drop(inner);
    cond.wait(mtx);
    0
}

pub fn sys_condvar_signal(cond_id: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let cond = inner.condvar_list[cond_id].as_ref().unwrap().clone();
    drop(inner);
    cond.signal();
    0
}

pub fn sys_enable_deadlock_detect(is_enable: i32) -> isize {
    if is_enable != 0 && is_enable != 1 {
        return -1;
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    inner.deadlock_enabled = is_enable == 1;
    0
}
