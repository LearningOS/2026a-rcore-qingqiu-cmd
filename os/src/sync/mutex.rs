//! Mutex (spin-like and blocking(sleep))

use super::UPSafeCell;
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
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

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self);
    /// Unlock the mutex
    fn unlock(&self);
    ///获取持有者
    fn get_owner(&self) -> Option<usize>;
    /// TID是否在等待队列
    fn is_waiting(&self,tid: usize) -> bool;
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
    owner: UPSafeCell<Option<usize>>,
    waiters: UPSafeCell<Vec<usize>>,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
            owner: unsafe { UPSafeCell::new(None) },
            waiters: unsafe { UPSafeCell::new(Vec::new()) },
        }
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) {
        trace!("kernel: MutexSpin::lock");
        let tid = current_tid();
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                // 加入等待队列
                let mut wq = self.waiters.exclusive_access();
                if !wq.contains(&tid) {
                    wq.push(tid);
                }
                drop(wq);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
                *self.owner.exclusive_access() = Some(tid);
                // 拿到锁，从等待队列移除自己
                let mut wq = self.waiters.exclusive_access();
                if let Some(pos) = wq.iter().position(|&x| x == tid) {
                    wq.remove(pos);
                }
                drop(wq);
                return;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        let mut locked = self.locked.exclusive_access();
        *locked = false;
        *self.owner.exclusive_access() = None;
    }
    fn get_owner(&self) -> Option<usize> {
        *self.owner.exclusive_access()
    }
    fn is_waiting(&self, tid: usize) -> bool {
        self.waiters.exclusive_access().contains(&tid)
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
    owner: Option<usize>,
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new() -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                    owner: None,
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) {
        trace!("kernel: MutexBlocking::lock");
        let tid = current_tid();
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
            mutex_inner.owner = Some(tid);
        }
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            let new_owner_tid = waking_task.inner_exclusive_access().res.as_ref().unwrap().tid;
            mutex_inner.owner = Some(new_owner_tid);
            wakeup_task(waking_task);
        } else {
            mutex_inner.locked = false;
            mutex_inner.owner = None;
        }
    }
    fn get_owner(&self) -> Option<usize> {
        self.inner.exclusive_access().owner
    }

    fn is_waiting(&self, tid: usize) -> bool {
        self.inner
            .exclusive_access()
            .wait_queue
            .iter()
            .any(|t| t.inner_exclusive_access().res.as_ref().unwrap().tid == tid)
    }
}
