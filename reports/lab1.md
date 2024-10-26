# 简答作业

1. 正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (`ch2b_bad_*.rs`) ），描述程序出错行为，同时注意注明你使用的 sbi 及其版本。

    **答：** 使用的 sbi 与版本：`RustSBI-QEMU Version 0.2.0-alpha.3`。

    各程序的出错行为：

    - `ch2b_bad_address.rs`：试图向`0x0`地址处写入`u8`类型的值`0`，使得处理器产生内存页错误`PageFault`，陷入内核`Trap`中，内核杀死该进程作为应答。

    - `ch2b_bad_instructions.rs`：试图执行 S 态特权指令`sret`，处理器检测到非法指令`IllegalInstruction`，陷入内核`Trap`中，内核杀死该进程作为应答。

    - `ch2b_bad_register.rs`：试图执行 S 态特权指令`csrr`访问处理器CSR`sstatus`，处理器检测到非法指令`IllegalInstruction`，陷入内核`Trap`中，内核杀死该进程作为应答。

2. 深入理解 trap.S 中两个函数 `__alltraps` 和 `__restore` 的作用，并回答如下问题:

    1. L40：刚进入 `__restore` 时，`a0` 代表了什么值。请指出 `__restore` 的两种使用情景。

        **答：** 刚进入 `__restore` 时，`a0` 代表经过`trap_handler`处理后的`TrapContext`。

        `__restore` 的两种使用情景：

        - 程序进行系统调用完毕，恢复程序运行并将系统调用的结果返回给程序。

        - 程序结束运行（正常结束或遇到处理器错误退出）或处理器时钟中断触发，内核切换运行下一个程序，恢复下一个程序的运行。

    2. L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。

        ```
        ld t0, 32*8(sp)
        ld t1, 33*8(sp)
        ld t2, 2*8(sp)
        csrw sstatus, t0
        csrw sepc, t1
        csrw sscratch, t2
        ```

        **答：** 从 `TrapContext` struct 中分别读取 `sstatus`、`sepc`和`sscratch` 的值到临时寄存器 `t0`、`t1`和`t2` 中，然后使用 `csrw` 指令分别将经过 `trap_handler` 处理后的 `sstatus`、`sepc`和`sscratch` 的值恢复到对应的 CSR 中去。