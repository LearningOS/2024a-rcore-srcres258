# 编程作业

实现的功能：

1. 在 `TaskControlBlock` 的实现中新增 `spawn` 方法用于新建子进程，并以此为基础实现 `sys_spawn` 系统调用。

2. 为代码中已有的 `TaskInfo` struct 新增 `priority` 和 `stride` 成员，并修改 `TaskManager` 的 `fetch` 实现改为按照 stride 优先级算法进行进程调度。又修改 `run_tasks` 函数，在每次进程调度前累加进程的 `stride` 以使得 stride 优先级算法能够正常运行。最后在上述既有算法逻辑的基础上实现 `sys_set_priority` 系统调用。

# 问答作业

stride 算法深入

stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。

- 实际情况是轮到 p1 执行吗？为什么？

    **答：** 并非轮到 p1 执行。因 `stride` 为无符号8位整数类型（即`u8`），此时p1.stride = 255, p2.stride = 250，p2 时间片执行完毕后加上 pass = 10 后大于 `u8` 类型的最大值 255，值溢出，发生环绕（wrapping），此时其实际值为 250+10-256=4。此时操作系统调度进程，因p1.stride = 255，p2.stride = 4，p1.stride > p2.stride，故操作系统仍然会选择 p2 进行调度而非 p1。

我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。

- 为什么？尝试简单说明（不要求严格证明）。

    **答：** 因进程优先级 priority >= 2，故 pass = BigStride / priority <= BigStride / 2，而每个进程的初始 stride 为 STRIDE_MIN，其后的 stride 均为 pass 累加，故若存在 STRIDE_MAX，则必有其差值 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。

- 已知以上结论，考虑溢出的情况下，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 partial_cmp 函数，假设两个 Stride 永远不会相等。

    ```rust
    use core::cmp::Ordering;

    struct Stride(u64);

    impl PartialOrd for Stride {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            // ...
        }
    }

    impl PartialEq for Stride {
        fn eq(&self, other: &Self) -> bool {
            false
        }
    }
    ```

    TIPS: 使用 8 bits 存储 stride, BigStride = 255, 则: `(125 < 255) == false`, `(129 < 255) == true`.

    **答：** 

    ```rust
    use core::cmp::Ordering;

    struct Stride(u64);

    impl PartialOrd for Stride {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            if self.0.wrapping_sub(other.0) < (1 << 63) {
                Some(Ordering::Less)
            } else {
                Some(Ordering::Greater)
            }
        }
    }

    impl PartialEq for Stride {
        fn eq(&self, other: &Self) -> bool {
            self.0.wrapping_sub(other.0) == (1 << 63)
        }
    }
    ```

# 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    > 帮助了学员[XBearH](https://github.com/XBearH/)解决了实验环境配置问题以及作业与实验报告的撰写与提交方面的问题，亦提供了基于本人经历的相关辅导与支持。但在这些过程中我保证未尝违反课程纪律、逾越有损他人学习的红线；我仅提供了口头原理层面的指点迷津，未提供自己的学习与实验成果供他人参看。

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    > 除在编写部分代码时参阅了 [Rust 官方 API 文档](https://doc.rust-lang.org/core/)外，未参阅任何其他人的代码实现（所有代码均为本人独立编写）。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。