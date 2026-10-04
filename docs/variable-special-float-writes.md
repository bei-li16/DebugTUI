# Watch／Locals 的特殊浮点写入

本批只适配声明普通 RAM 的可赋值 `float`／`double` 变量及成员。它不开放系统、VFP 状态或 S/D/Q 寄存器 writer，也不改变目标 FPU 控制、模式或调试权限。

Float 输入支持正负 Infinity／NaN；Bytes 输入可以指定完整 32／64 位 NaN 符号、quiet／signaling 位和 payload。溢出的有限输入仍拒绝；显示的十六进制原始位始终是最终写入和回读的依据。

## 写入路径与失败行为

Preview 和 Apply 都读取真实类型、大小、可赋值属性、地址、RAM 声明、owner、线程、帧及停止代次。检查和赋值期间临时禁止 GDB 调用目标函数，并恢复原设置；恢复失败使通道进入 FAULT。

首先由 GDB 求值只含常量的浮点表达式，通过临时 varobj 的 hexadecimal 格式核对位模式。算术 NaN 不保证符号或 payload；不匹配时，按 `show endian` 的当前目标字节序使用 GDB 自带 Python 的 `gdb.Value(buffer, type)` 创建主机值，并保存在本请求独占命名的 convenience variable 中。固定内置类型的大小和整个位模式都必须再核对，之后才允许 Apply 发送一次 `-var-assign` 类型赋值。相关 API 来自 [GDB Python Value 文档](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Values-From-Inferior.html)。不会通过目标函数、分配目标缓冲区或原始地址写内存构造 NaN。

Apply 会重新创建常量，检查完常量后再次核对停止上下文，再发送唯一赋值。创建成功的临时 varobj 和自有主机常量均尝试清理；已有同名 convenience variable 安全拒绝，不删除用户值。Preview、Cancel、能力缺失、位模式不符、未知字节序或探测期间进入 RUNNING 均不发送赋值。发送后的结果分别保留 verified、mismatch、accepted 或 unknown；清理失败不能把已经发生的写入描述成未发送。草稿消费后不重放，也没有内存写入回退。

## 本地 GDB 的实际限制

工作区配套 `tools/bin/gdb/bin/arm-none-eabi-gdb.exe` 为 `14.2.90.20240526-git`，本地实测没有 Python 支持。该版本在加载 Cortex-R52 EABI 对象后，正负 float／double Infinity 的大小端常量均通过核对；需要精确 buffer 的 NaN 路径会在发送赋值之前拒绝。不能把本机 MinGW GDB 的成功结果记为配套 ARM GDB 的 NaN 写入能力。完整工具集整合仍须提供经过验证的 Python GDB 或其他精确主机值后端，当前未安装或替换工具集。

ARM GDB 未加载 ELF 时可能选用旧 ARM FPA 浮点布局；离线检查必须提供实际目标 ABI 的 ELF／对象，不能把主机默认格式当作板级证明。运行时按实际 GDB 位模式核对，不匹配则停止。

```powershell
# 只加载 ELF／对象，不启动 inferior，不连接板卡。
node scripts/test-float-literals-gdb.cjs --elf <Cortex-R52-EABI-object>
node scripts/test-float-literals-gdb.cjs --gdb <Python-enabled-GDB> --elf <matching-debug-ELF>
```

## 软件证据与延后案例

`scripts/test-variable-write-gdb.cjs` 在实际本机 GCC/GDB 进程中执行 9 个阶段，覆盖 32／64 位正负 Infinity、quiet／signaling NaN payload、一次类型赋值、独立整数位读取、两侧哨兵、目标函数调用计数和临时值清理。`tests/write_access/float_cases.rs` 使用严格 MI 失败模型补充目标大小端、能力缺失、Apply 重新检查、异步 RUNNING、发送后结果、无重试和 Scope All 的当前核／peer 隔离；模型不是实际 GDB API 或上板证明。

[延后测试说明](../tests/cases/variable-special-floats.md) 与两个 JSON 模板准备了 WRITE-H01 特殊 float／double 子集。驱动默认 skipped；必须按真实 ELF、实际 RAM、owner 和字节序填写工程，显式 `--run --special-floats` 才执行。上板测试按任务要求未执行。位域、引用、long double 和 128 位实际 typed writer 仍待完成；完整 TODO 没有因本批通过而完成。
