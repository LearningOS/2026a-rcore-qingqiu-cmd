# Lab1 实验报告：sys_trace 系统调用实现

## 一、实验需求

在 ch3 分时多任务内核中新增系统调用 `sys_trace`（ID 410），用于追踪当前任务系统调用的历史信息。

## 二、模块化设计

### 2.1 数据存放位置：TaskControlBlock

计数的数据属于“当前任务”，天然应挂在 `TaskControlBlock`（TCB）上。每个任务有独立的调用历史，不能放在全局。


###2.2 计数时机：syscall 分发入口

计数的重要要求是“本次调用也计入统计”。如果计数散落在各个系统调用的实现里，会遗漏、会重复，且 trace_request=2 查询时容易少算。

统一在 syscall 分发函数的最开头 +1，是最稳妥的设计。


##三、关键问题与解决

###3.1 借用生命周期：inner 必须显式 drop

问题：TASK_MANAGER.inner 是 UPSafeCell<TaskManagerInner>，exclusive_access() 在运行期借用检查。若在 syscall 开头借了 inner，然后调用 process::sys_trace，后者又 exclusive_access()，会 panic：already borrowed。

解决：用 {} 作用域显式控制借用生命周期。计数完成后立即离开作用域，RefMut 自动 drop，借用释放；进入 match 分发时再重新借。
也可以用drop(inner)来实现。

###3.2 计数顺序：先加一再查询

trace_request=2 查询 syscall_count[id] 时，本次 trace 调用必须已计入。题目要求“本次调用也计入统计”。

如果先查询再计数，得到的次数会少 1。因此计数动作必须在 match 分发之前完成。当前设计天然满足：syscall 开头统一 +1，sys_trace 的 2 号分支直接读，读到的是含本次的值。

###3.3 越界保护

syscall_count[syscall_id] 前加 if syscall_id < MAX_SYSCALL_NUM。

##四、编译期问题与修复

整个过程遇到的编译错误可以分为三类。

###4.1 拼写错误：SYSYCALL → SYSCALL

报错：

```
error[E0432]: unresolved import `crate::task::MAX_SYSYCALL_NUM`
help: a similar name exists in the module: `MAX_SYSCALL_NUM`
```
原因：手写常量名时多打了一个 Y。

###4.2 可见性：pub 缺失

报错：

```
error[E0616]: field `inner` of struct `TaskManager` is private
error[E0425]: cannot find value `MAX_SYSCALL_NUM` in this scope
```

原因：

1. TaskManager.inner 原定义未加 pub，syscall/mod.rs 跨模块访问不到；
2. MAX_SYSCALL_NUM 定义在 task/task.rs，但 task 模块是私有的，外部看不到。

修复：

1. TaskManager.inner 加 pub；
2. 在 task/mod.rs 加 pub use task::MAX_SYSCALL_NUM; 重导出。

判断规则：跨模块访问 pub 项，若中间模块不 pub，要在中间模块 pub use 转一下。


###4.3 文档缺失：deny(missing_docs)

报错：

```
error: missing documentation for a constant
note: the lint level is defined here: #![deny(missing_docs)]
```

原因：框架在 main.rs 顶部开了 #![deny(missing_docs)]，所有 pub 项必须有 /// 文档注释，否则视为错误而非警告。



##五、框架与本机环境不兼容问题

###5.1 现象

编译通过、直接 make run，QEMU 输出停在：

```
[RustSBI] INFO  - Boot HART ID : 0
```

之后没有任何 [kernel] 输出，30 秒后 QEMU 被 timeout 杀掉。

###5.2 排查过程

第一步，git stash 暂存所有改动，用原始框架代码跑，结果同样卡在 Boot HART ID : 0。由此确定：不是 sys_trace 的实现问题，是 bootloader 与本地 QEMU 环境的兼容性问题。

第二步，比对两个 bootloader，发现两者版本不同，PMP 配置不同。

###5.3 根因：RustSBI 0.4.1 的 PMP 切碎了内核区域

RustSBI 0.4.1 的 PMP 配置：

```
pmp1: 0x8000000..0x8000000  RWX
pmp2: 0x8000000..0x8000000  RWX
pmp3: 0x8000000..0x8028000  R      ← 只读
pmp4: 0x8028000..0x8036000  NONE   ← 禁止访问
pmp5: 0x8036000..0x8071000  RW     ← 不可执行
pmp6: 0x8071000..0x8800000  RWX
```

内核链接在 0x80200000。ch3 把所有用户应用嵌入内核二进制，内核体积远超 0x28000（160KB）。执行一旦越过 0x8028000，立即撞上只读/禁止区，CPU 触发异常，且此时 S 态异常处理尚未就绪，表现为静默死机。

对比 RustSBI 0.3.0-alpha.2：

```
pmp03: 0x80200000..0x88000000 (xwr)   ← 整个内核区域 RWX
```

一整块宽松映射，ch3 内核无论多大都在可执行范围内。

###5.4 解决

用 0.3.0 替换作业仓库的 0.4.1：

```bash
cp ~/rCore-Tutorial-Code/bootloader/rustsbi-qemu.bin bootloader/rustsbi-qemu.bin
```

替换后 make run BASE=1 立即恢复输出，make run BASE=0 三个实验测例全过。


###5.5 最终代码文件提交构成

```
bootloader/rustsbi-qemu.bin   ← 0.3.0，解决 PMP 不兼容
os/src/task/task.rs           ← 新增字段和方法
os/src/task/mod.rs            ← 字段初始化 + 常量重导出
os/src/syscall/mod.rs         ← 计数 + 410 分发
os/src/syscall/process.rs     ← sys_trace 三分支
reports/lab1.md               ← 本文
```
本地 CI 验证结果，最终7 项期望输出全部命中，sys_trace 的读、写、查询三种功能与 sleep 系列基础功能均正常。


#简答作业：
1. 三个bad测例
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
出错原因是访问地址0x0，内核打印bad_address和出错指令，杀掉改应用并切换下一个;
[kernel] IllegalInstruction in application, kernel killed it.
出错原因是执行非法指令，内核打印信息杀掉应用;
[kernel] IllegalInstruction in application, kernel killed it.
出错原因是在U态下执行S态特权指令，被判定为非法，触发Trap,内核打印信息杀掉应用。
SBI版本：RustSBI 0.3.0-alpha.2
框架自带的0.4.1在本机因为pmp配置不兼容无法启动。

2. __alltraps 和 __restore
1.L40:刚进入__restore时，sp代表调用trap_handler前内核栈的栈顶，a0是TrapContext的起始地址，在执行mv sp,a0之后,sp才指向TrapContext;
两种使用情景：
首次运行应用，可以看成是从一个内核态返回，返回之后的sp就是用户栈，即TrapContext的首地址；
Trap处理完返回：先是__alltraps调用trap_handler，返回之后a0就是TrapContetx指针，进入__restore；
2.L43-L48:特殊处理了sstatus、spec、sscratch,它们都是CSR不能直接ld到目标寄存器，必须用t0t1t2中转,用ccrw写入；
sstatus记录了在Trap之前的特权级状态，从用户态进入Trap会保存SPP=0,这样返回就能保证特权级正确；
spec记录了在进入Trap之前的最后一条指令，sret后PC就会跳到这里，在Trap中会自动变成用户态的下一条指令地址；
sscrach和sp会交换内容，现在sscrach保存的是用户栈指针，Trap结束后它与sp再交换，使sp恢复成用户栈；
3.L50-L56:x2存放sp，在下面特殊处理；x3存放线程指针，用户态程序不使用。
4.L60:在__restore中，该指令之后sp存放用户栈指针，sscratch指向内核栈指针；
5.状态切换发生在csrw sstatus,t0;此时sstatus就是U态，后面sret后进入U态；
6.L13:在__alltraps中，该指令之后sp存放内核栈指针，sscratch指向用户栈；
7.在用户态ecall之后会触发Trap,在__alltraps中会发生CSR（sstatus）的状态切换。


#荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

无

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
