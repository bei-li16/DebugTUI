# C++ 引用与 128 位标量写入

Watch／Locals 的统一 typed writer 现支持声明普通 RAM 中的可赋值 C++ lvalue／rvalue reference，以及编译器和实际 GDB 均提供的 128 位整数标量。引用到指针修改的是指针变量，不修改 pointee。const／volatile 根对象及 referent 仍拒绝，long double 和位域尚未适配；CPU 状态、银行及 S/D/Q 向量 writer 不由本批开放。

## 类型、存储与写入

预览和应用保留 GDB `ptype /r` 的实际引用类型，并仅剥离最外层 `&`／`&&` 来判断 referent 的整数、浮点、布尔或指针类型和限定符。对引用使用 `__typeof__(*(&(expression)))` 得到被引用对象类型，避免把数字强转成 C++ reference。实际 `&(expression)` 必须解析到声明 RAM 的 referent 地址，并验证实际 `sizeof`、访问宽度／对齐、owner、线程、栈帧、停止代次、ELF、可赋值属性及 `may-write-memory`。没有实际 RAM 地址的引用拒绝；不会把 reference 绑定槽、优化值或 saved register 推断成可写 referent。

实际本机 GDB 允许取得位域地址，6 位字段的 `sizeof` 也返回 4 字节容器大小，地址和大小不能证明普通标量布局。Preview／Apply 对成员路径额外读取直接父类型的 `ptype /r`，要求字段能对应唯一的普通直接声明；位域、继承或无法确认的成员拒绝赋值。模板参数中的 `const` 或 `*` 不视为外层限定符或指针。该校验是当前不支持位域时的拒绝路径，尚未实现 DWARF bitpos／bitsize 和邻接位验证。

原有 `-var-assign` 类型赋值路径保持不变：服务锁覆盖检查、一次赋值和回读，临时禁止目标函数调用并恢复设置；没有裸地址写入回退。Apply 重新创建对象、检查引用类型和 referent 地址，切核、切帧、地址或权限变化都会使原草稿不可应用。只消费一次草稿，发送后错误不重试或自动恢复旧值；Scope All 仅作用于当前 owner。浮点引用复用已有原始位核对和条件 Python buffer 路径。

128 位表达式原先硬编码 `unsigned __int128`；本机 GDB 14 可以识别 DWARF 的 16 字节对象，却无法解析该类型名。现由实际对象的 `__typeof__` 构造高 64 位／低 64 位常量，Preview 和 Apply 都通过临时 varobj 的 hexadecimal 格式核对完整 128 位。截断、类型不支持、位模式不符、清理失败或探测期间进入 RUNNING 均在唯一赋值之前拒绝。signed 输入覆盖最小／最大值与 -1，unsigned 输入不能超过实际 signed 类型的正范围。

## 证据与范围

`scripts/test-variable-reference-gdb.cjs` 的实际本机 C++ GCC/GDB 八阶段覆盖全局／局部／成员／聚合引用、右值引用、const／volatile 拒绝、指向 const 的指针引用、精确 NaN 浮点引用、真实 signed／unsigned 128 位高低字和范围，以及位域 Preview 拒绝。通过独立 referent／整数位读取、位域完整容器与两侧哨兵、函数计数、草稿取消／重用与帧切换检查，不把规划器单元结果当作后端能力。完整 Cargo 回归 312 单元、97 集成通过，2 ignored，严格 Clippy 通过；记录在 `artifacts/functional-1791157039720-3cb3697e/report.json`。该报告选取 unit／variable-write／variable-reference 三 suites，其他 19 suites 未选。

严格 MI 模型另覆盖引用地址／类型／权限变化、实际 RAM 归属与 Scope All peer 保持、128 位截断／能力变化、清理／RUNNING／回读失败和结果未知。引用与 128 位的延后驱动均在实际 DebugTUI 二进制上验证独立 RAM 读回、两侧哨兵和 verified 后显式恢复；报告明确 `board_tests_executed=false`。

`tests/fixtures/variable-reference-board.cpp` 已分别用本机 64／32 位 GCC 和 GNU Arm C++ 编译。R52 参数为 `-mcpu=cortex-r52 -mfpu=neon-fp-armv8 -mfloat-abi=softfp -g -O0`，产物为 ARM ELF32 EABI v5。配套 ARM GDB 无 Python，但能在实际对象中解析 `unsigned int &`、referent 的 4 字节大小和生成的 UInt32 常量；离线检查不发送目标赋值。该 Arm 编译器的 `__SIZEOF_INT128__` 缺失，fixture 明确报告 `reference_fixture_has_int128=0`，不能把本机 128 位证明外推为 R52 标量或向量写入能力。

```powershell
# 只加载已编译的 R52 对象，不启动 inferior 或连接板卡。
node scripts/test-variable-reference-arm.cjs --object <variable-reference-board-arm.o>
```

上板状态和实际 core0／core1／shared／局部帧／128 位适用性 case 见 [延后矩阵](../tests/cases/variable-references-wide.md)，位域拒绝与尚未实现的写入案例见 [位域案例](../tests/cases/variable-bitfields.md)。当前未执行上板、推送或发布。位域仍需 DWARF bitpos／bitsize、容器实际范围与邻接位事务验证，继承成员映射也未适配；完整 TODO 中的其他读写类别、工具集整合、最终回归和 Release 保持待完成。
