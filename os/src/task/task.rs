//! Types related to task management

use super::TaskContext;
///系统调用最大数量
pub const MAX_SYSCALL_NUM: usize = 500;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The count of syscall
    pub syscall_count: [usize;MAX_SYSCALL_NUM],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}

impl TaskControlBlock {
	///获取当前任务系统调用次数
	pub fn get_syscall_count(&self, id: usize) -> usize {
		self.syscall_count[id]
	}
	///系统调用次数加一
	pub fn inc_syscall_count(&mut self,id: usize) {
		self.syscall_count[id] += 1;
	}
}
