# Watch／Locals 位域写入

统一 typed writer 支持普通 RAM 中、实际 DWARF 布局可确认的直接整数／布尔位域。字段宽度为 1–64 位，声明类型为 1／2／4／8 字节，位域必须适合 GDB 的 64 位写入字。使用一次 `-var-assign` 修改字段；不会生成裸地址写入回退或调用目标函数。const／volatile、未知／继承／歧义布局、未声明或与目标不一致的字节序、MMIO／Flash、无实际父地址、超范围写入字及过期上下文拒绝。

GDB 允许某些位域取地址，`sizeof(field)` 又是声明整数类型的字节数，二者均不能证明字段位宽。Preview／Apply 先核对父类型的直接声明，再用 `ptype /rod parent` 取得目标调试信息的字段 byte:bit offset、bitsize、声明类型大小及父大小；与独立 `sizeof` 比较。实际 `show endian` 必须与 RAM policy 的 `little_endian` 一致。详见 [GDB 的字段布局命令](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Symbols.html)。此路径不依赖 GDB Python。

元数据保留父表达式／类型、实际父 RAM 地址、完整 owner／scope、布局与字节序。`scalar.bits` 是实际字段宽度，`declared_bits` 是声明整数类型宽度。signed hexadecimal 值必须符合声明类型的符号扩展，再规范为字段原始位；输入越界在发送前拒绝。Preview 对原始父字节提取的值和 GDB 字段值做一致性核对，不把声明类型的 32 位值当成 6 位字段。

Apply 在服务锁内重新创建实际对象、复核所有元数据和权限，并在赋值前读取新鲜父字节。范围包括整个父对象，以及 GDB 对齐访问可能扩展到的额外邻接字节；必须全部位于同一声明 RAM 区间。GDB 14 的访问扩大规则可见 [value_assign 的内存位域分支](https://gnu.googlesource.com/binutils-gdb/+/refs/heads/gdb-14-branch/gdb/valops.c)。GDB 自己按当前值执行类型赋值，DebugTUI 不发送 Preview 时保存的旧父字。

唯一赋值之后，分别读取字段和完整父／邻接字节。仅请求字段位变化且其他位与 Apply 前新鲜值一致时为 `verified`；字段匹配但邻接字节变化仍为 `mismatch`。回读失败保留 `accepted`，发送后错误为 `unknown` 并隔离共享通道；不重试、重放或自动恢复旧值。`observed` 使用独立父字节提取的实际字段值，`gdb_observed` 保留类型对象读值；`neighbours_preserved` 只比较目标字段之外的位，`parent_matches_expected` 比较完整预期父字节，二者分别解释字段写错或邻接变化。结果同时记录 `parent_before_bytes`、`parent_expected_bytes`、`parent_after_bytes`，写后使用统一失效流程。Scope All 只作用于当前 owner；shared RAM 沿用相关核心全部暂停的门禁。

## 软件与离线证据

实际本机 GCC/GDB 测试覆盖 unsigned 5／7／14／64 位、signed 6／63 位最小／最大值、packed 不对齐且跨字节字段、越界、取消、只读限定符、邻接／保留位、函数计数和原项目保持。严格 MI 模型另覆盖大小端、Preview 后邻接位变化、布局／权限／owner 变化、布局与原始值不符、RUNNING、回读失败、结果未知、清理失败、一次赋值和双核 Scope All peer 保持。

完整 Cargo 回归 315 单元、103 集成通过，2 ignored；严格 Clippy 通过。功能报告 `artifacts/functional-1791160367322-c3976d3d/report.json` 选取 unit／variable-write／variable-reference／variable-bitfield，三个实际 native suites 分别 9／8／6 阶段通过，F26 映射齐全；其他 19 suites 未选，不能据此认定完整交付回归完成。

`tests/fixtures/variable-bitfield-board.cpp` 已用 GNU Arm 11.4 以 Cortex-R52／NEON／softfp 参数分别编译大小端 ARM ELF32 EABI v5 对象。配套无 Python ARM GDB 的离线测试比较真实 DWARF offset 与原始节字节，signed 6 位 -2 的原始值为 62；packed 父大小为 5 字节，本机 Windows ABI 为 6 字节，不能套用主机布局。该离线检查不执行目标赋值。

```powershell
node scripts/test-variable-bitfield-gdb.cjs target/debug/debugtui.exe
node scripts/test-variable-bitfield-arm.cjs --little-object <R52-little.o> --big-object <R52-big.o>
node scripts/test-variable-bitfield-hardware.cjs
```

延后驱动默认 4 skipped，不访问目标。环境就绪时填写独立审核的字段 layout、核心／owner、调用帧、声明 RAM 与哨兵，显式 `--run` 才执行取消、单次类型赋值、独立父字节／字段／哨兵检查和 verified 后的显式恢复。驱动不运行、暂停或复位 CPU。实际 DebugTUI 二进制＋软件 MI 模型五阶段通过，报告 `board_tests_executed=false`。详见 [BF-H 案例](../tests/cases/variable-bitfields.md) 和 [case 模板](../tests/fixtures/variable-bitfield-board.example.json)。所有上板项目仍未执行，完整 TODO、其他 writer、工具集和最终发布仍待完成。
