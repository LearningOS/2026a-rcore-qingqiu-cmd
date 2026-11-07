## 一、编程总结

本次实验在 ch5 进程机制上完成四项工作：

1. **迁移 `sys_get_time`/`sys_mmap`/`sys_munmap`**：把 ch4 的 `TASK_MANAGER.inner.exclusive_access().current_task` + `tasks[cur]` 改为 `current_task().unwrap().inner_exclusive_access()`，其余逻辑不变，`translated_refmut` 直接复用。

2. **实现 `sys_spawn`**：`translated_str` 读 path → `get_app_data_by_name` 找 ELF → `TaskControlBlock::new(data)` 新建地址空间 → 设 `parent`/`children` → `add_task`，**不复制父进程地址空间**。

3. **实现 stride 调度**：TCB 加 `stride`/`pass`/`priority`（初始 `0`/`BIG_STRIDE/16`/`16`），`fetch` 选 stride 最小，`run_tasks` 里 `stride += pass`，新增 `sys_set_priority`（`prio < 2` 返回 -1）。

4. **`sys_waitpid` 改阻塞**：包 `loop`，没僵尸子进程时 `drop(inner)` + `suspend_current_and_run_next()`，让出 CPU 等待。

## 二、问答题

### 1. stride 溢出问题

此时并不是轮到p1执行，因为p2的stride大小溢出，实际上被识别为4,4<255。

### 2. priority >= 2 的保证

设stride的取值范围是[mn,mx];(有mx-mn<=B/2)
在初始状态，STRIDE_MAX – STRIDE_MIN =0 < BigStride / 2。
此时选中stride最小进程A，更新后分两种情况：
这里pass = BigStride / prio <= Bigstride / 2
若mn+A.pass<=mx,则max'=mx,min>=min,max'-min'<=mx-mn<=B/2;
若mn+A.pass>mx,则max'=mn+A.pass,min>=mn,max'-min'<=A.pass<=B/2。
得证

### 3. 溢出安全的比较器

由于存储类型是u64,且步长不会大于“半圈”，可以认为作模减法的前半圈小数字说明self大于other;后半圈大数字说明self小于other;
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        //模减法比较
	let diff = self.0.wrapping_sub(other.0);
	if diff == 0{
		None
	} else if diff < (1u64 << 63) {
		Some(Ordering::Greater);
	} else {
		Some(Ordering::Less);
	}
    }
}

//说明不允许相等
impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}


## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

无

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
