//! Semaphore

use crate::sync::UPSafeCell;
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc, vec::Vec};

fn current_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

/// inner of semaphore
pub struct SemaphoreInner {
    /// current count (negative means waiting)
    pub count: isize,
    /// initial resource count
    pub total: usize,
    /// waiting queue
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
    /// per-tid holding count
    pub alloc: Vec<(usize, usize)>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    total: res_count,
                    wait_queue: VecDeque::new(),
                    alloc: Vec::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        let old_count = inner.count;
        inner.count += 1;
        let cur_tid = current_tid();

        // 调用者释放资源，alloc 减 1
        if let Some((_, n)) = inner.alloc.iter_mut().find(|(t, _)| *t == cur_tid) {
            if *n > 0 {
                *n -= 1;
                if *n == 0 {
                    inner.alloc.retain(|(t, _)| *t != cur_tid);
                }
            }
        }

        // 有等待者，唤醒一个并把资源转给它
        if old_count < 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                let wake_tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
                if let Some((_, n)) = inner.alloc.iter_mut().find(|(t, _)| *t == wake_tid) {
                    *n += 1;
                } else {
                    inner.alloc.push((wake_tid, 1));
                }
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self) {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
            // 被唤醒时 alloc 已在 up 里转好，不再加
        } else {
            let tid = current_tid();
            if let Some((_, n)) = inner.alloc.iter_mut().find(|(t, _)| *t == tid) {
                *n += 1;
            } else {
                inner.alloc.push((tid, 1));
            }
        }
    }

    /// Get current count
    pub fn get_count(&self) -> isize {
        self.inner.exclusive_access().count
    }

    /// Get initial total count
    pub fn get_total_count(&self) -> usize {
        self.inner.exclusive_access().total
    }

    /// Whether a tid is waiting
    pub fn is_waiting(&self, tid: usize) -> bool {
        self.inner
            .exclusive_access()
            .wait_queue
            .iter()
            .any(|t| t.inner_exclusive_access().res.as_ref().unwrap().tid == tid)
    }

    /// Get allocation snapshot
    pub fn get_alloc(&self) -> Vec<(usize, usize)> {
        self.inner.exclusive_access().alloc.clone()
    }

    /// Get (alloc, wait_tids) snapshot
    pub fn get_snapshot(&self) -> (Vec<(usize, usize)>, Vec<usize>) {
        let inner = self.inner.exclusive_access();
        let alloc = inner.alloc.clone();
        let wait_tids = inner
            .wait_queue
            .iter()
            .map(|t| t.inner_exclusive_access().res.as_ref().unwrap().tid)
            .collect();
        (alloc, wait_tids)
    }
}
