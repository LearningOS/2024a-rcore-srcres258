# 编程作业

实现的功能：

1. 为每个进程的 `ProcessControlBlockInner` 分别新增了算法所需的 Available 向量、Allocation 矩阵和 Need 矩阵。

2. 在 Mutex 和 Semaphore 的资源申请和释放过程中为当线程更新上述三个向量和矩阵，判断是否产生 deadlock，若产生 deadlock 则与此同时是否已经开启了 deadlock 的检测功能。若 deadlock 发生且检测开启，则拒绝本次 Mutex 或 Semaphore 的资源申请并返回 `-0xDEAD` 以说明检测到了 deadlock。

3. 最后修改 `sys_mutex_lock` 和 `sys_semaphore_down` 系统调用的部分逻辑以支持 deadlock 的检测功能并在 deadlock 发生时能够返回 `-0xDEAD`。然后完成 `sys_enable_deadlock_detect` 的实现以使用户程序能够按需开启或关闭当前进程的 deadlock 检测功能。

# 问答作业

1. 在我们的多线程实现中，当主线程 (即 0 号线程) 退出时，视为整个进程退出， 此时需要结束该进程管理的所有线程并回收其资源。

    - 需要回收的资源有哪些？

        **答：** 回收的资源主要有两类：一是分配给进程的内存空间，二是分配给进程打开的文件描述符。

    - 其他线程的 TaskControlBlock 可能在哪些位置被引用，分别是否需要回收，为什么？

        **答：** 其他线程的 TaskControlBlock 在内核的系统调用过程中可能通过调用 `current_task` 函数被临时引用到。因这些临时引用均在使用完毕后，或离开作用域了，或明式调用 `drop` 函数进行了销毁，故其已被自动回收，无需再在进程结束时考虑这些临时资源的回收。

2. 对比以下两种 Mutex.unlock 的实现，二者有什么区别？这些区别可能会导致什么问题？

    ```rust
    impl Mutex for Mutex1 {
        fn unlock(&self) {
            let mut mutex_inner = self.inner.exclusive_access();
            assert!(mutex_inner.locked);
            mutex_inner.locked = false;
            if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
                add_task(waking_task);
            }
        }
    }

    impl Mutex for Mutex2 {
        fn unlock(&self) {
            let mut mutex_inner = self.inner.exclusive_access();
            assert!(mutex_inner.locked);
            if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
                add_task(waking_task);
            } else {
                mutex_inner.locked = false;
            }
        }
    }
    ```

    **答：** 区别是 `Mutex1` 在唤醒其他等待该 `Mutex` 资源的线程前就将当前 `Mutex` 的上锁状态置为 `false`，而 `Mutex2` 是在确认尚无其他等待该 `Mutex` 资源的线程之后才将当前 `Mutex` 的上锁状态置为 `false`。

    `Mutex1` 这种操作的后果是若在下次被操作系统调度的另一个线程也尝试获取该 `Mutex` 资源（其尚未被加入到等待队列中进入等待状态），因上锁状态已被置为 `false`，故其将成功获取该锁；但后续因在 `Mutex1` 的 `unlock` 过程中被唤醒的线程也已经被加入到了操作系统的后续线程调度队列中，故其也将被操作系统调用得以运行。此时就出现了大于两个线程同时获取到该 `Mutex` 资源的情况，而这种情况显然是违背了 `Mutex` 的设计原则的；故这种 `unlock` 的实现方式是不安全也不可取的。

    因此需要改用更安全的 `Mutex2` 的 `unlock` 的实现。该实现确认尚无其他等待该 `Mutex` 资源的线程之后才将当前 `Mutex` 的上锁状态置为 `false`，进而避免先前早已加入操作系统的后续线程调度队列的线程获得到 `Mutex` 资源的情况，永远不会出现大于两个线程同时获取到该 `Mutex` 资源的情况。

# 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    > 帮助了学员[XBearH](https://github.com/XBearH/)解决了实验环境配置问题以及作业与实验报告的撰写与提交方面的问题，亦提供了基于本人经历的相关辅导与支持。但在这些过程中我保证未尝违反课程纪律、逾越有损他人学习的红线；我仅提供了口头原理层面的指点迷津，未提供自己的学习与实验成果供他人参看。

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    > 除在编写部分代码时参阅了 [Rust 官方 API 文档](https://doc.rust-lang.org/core/)外，未参阅任何其他人的代码实现（所有代码均为本人独立编写）。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
