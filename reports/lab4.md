# Lab4 实验报告

## 一、编程总结

本次实验在 ch6 文件系统上完成三项工作：

1. **实现 `sys_fstat`**：从 `fd_table` 拿 `OSInode`，通过 `File::inode()` 拿到 `easy-fs` 的 `Inode`，读 `inode_id`/`nlink`/`is_dir`，构造 `Stat`（`dev=0`、`mode` 按 `DIR`/`FILE`、`nlink`），用 `translated_refmut` 写回用户空间。

2. **实现 `sys_linkat`**：`translated_str` 读新旧路径，同名返回 -1，`ROOT_INODE.find(old_name)` 找目标 inode，`ROOT_INODE.link(new_name, inode_id)` 在根目录加目录项并把目标 inode 的 `nlink` 加一；`link` 内部用 `increase_size` 扩展目录的 size，确保新目录项在 `find` 的可搜索范围内。

3. **实现 `sys_unlinkat`**：`translated_str` 读路径，`ROOT_INODE.unlink(name)` 删除目录项、`nlink` 减一，`nlink` 到 0 时清空 inode 和数据块。

**支撑改动**：`easy-fs` 的 `DiskInode` 加 `nlink` 字段、`Inode` 加 `inode_id` 字段和 `link`/`unlink`/`nlink`/`is_dir`/`inode_id` 方法；`File` trait 加 `inode()` 方法，`OSInode` 返回 `Some`，`Stdin`/`Stdout` 返回 `None`；ch5 的 stride 调度、阻塞 `waitpid`、`sys_get_time`/`mmap`/`munmap`/`spawn`/`set_priority` 全部迁移过来，保持前向兼容。


## 二、问答题：root inode 的作用与损坏后果

1. root inode 的作用：

整个文件系统的入口。所有路径查找都从根目录的 inode 开始。
它存储根目录的目录项（`ch6_usertest`、`filea` 等），通过它才能找到用户程序和其他文件。
`easy-fs` 的 `EasyFileSystem::root_inode` 返回 inode 编号 0 的 `Inode`，`ROOT_INODE` 全局变量就是它。

2. 如果 root inode 损坏：

无法访问文件系统——所有文件查找都从 root inode 开始，它坏了，用户无法访问任何文件或目录。
内核无法加载用户程序（程序在 `fs.img` 里），系统无法正常启动，shell 起不来。
最坏情况：文件系统完全不可用，只能重新格式化 `fs.img`。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

deepseek

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

rCore-Tutorial-GuideBook

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
