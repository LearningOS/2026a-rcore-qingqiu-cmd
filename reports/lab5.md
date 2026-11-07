# Lab5 实验报告

## 一、编程总结

本次实验在 ch8 多线程框架上实现死锁检测：

1. **`sys_enable_deadlock_detect`**：设置 `ProcessControlBlockInner.deadlock_enabled`。
2. **`MutexBlockingInner` 加 `owner`**，`Mutex` trait 加 `get_owner`/`is_waiting`，`lock`/`unlock` 维护 owner（unlock 时锁转让给被唤醒者）。
3. **`SemaphoreInner` 加 `total`/`alloc`**，`down`/`up` 维护 per-tid 持有计数；`up` 先减调用者的 `alloc`，再判断是否唤醒，被唤醒者继承资源。
4. **银行家算法**：`banker_check_sem`/`banker_check_mutex` 从快照构造 `Available`/`Allocation`/`Need` 矩阵，`available == 0` 时把当前请求的 `need` 置 1 后调 `is_safe`，不安全返回 `-0xDEAD`。
5. **`sys_get_time`** 从 ch7 迁移（用户库 `sleep` 的忙等依赖它，不实现会导致 `sleep` 死循环，`sem1` 卡住）。


## 二、问答题

### 1. 主线程退出时需要回收哪些资源？其他线程的 TCB 在哪被引用？

**要回收**：其他线程的 `KernelStack`、`TrapContext` 页（`trap_cx_ppn`）、`PidHandle`、`TaskContext`、进程的 `memory_set`、`fd_table`。

**TCB 被引用的位置**：`ProcessControlBlockInner.tasks`（进程持有）、`ready_queue`（调度器）、`Processor.current`（当前运行）、Mutex/Semaphore/Condvar 的 `wait_queue`。

**是否回收**：`tasks` 和 `wait_queue` 里的必须清理，否则 `Arc` 计数不归零，`KernelStack`/`pid`/`memory_set` 泄漏；`ready_queue`/`Processor.current` 由调度器负责。

### 2. 两种 Mutex 实现的区别

**`Mutex1` 正确**：`lock` 是 `loop`，被唤醒后重新检查 `locked`，抢不到继续等；`unlock` 先 `locked = false` 再唤醒。

**`Mutex2` 有 bug**：`lock` 没 `loop`，被唤醒后不重新检查 `locked`，直接返回；如果此时锁被别的线程抢了，两个线程同时认为自己持有锁，互斥失效，导致数据竞争。

`Mutex2::unlock` 有等待者时保持 `locked = true` 转让锁，必须配合 `lock` 里的 `loop` 重新检查才正确——但它没有 `loop`，所以错了。

## 三、荣誉准则

在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

deep seek

此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：
无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
