# WRITE-H01 引用与 128 位变量延后案例

状态：**SKIPPED／未执行上板测试**。本机 C++/GDB 和 MI 模型测试不作为板级成功证明。

将 `tests/fixtures/variable-reference-board.cpp` 以 `-g -O0` 编译、链接到实际声明的普通 RAM。固件停在 `debug_reference_fixture`；驱动不 run、halt 或 reset。工程预先加入 Watch，按真实 core／chip／cluster owner 和字节序填写 JSON 模板。private RAM 分别使用 core0、core1；shared RAM 另声明共享 owner，并暂停所有相关核心。不要把 host 上的 ABI、地址或 C++ public 子节点索引当作板级配置，先检查实际 ELF 中的树。

| Case | 操作及预期 | 状态 |
|---|---|---|
| RV-H01 | core0 的 `reference_value`：Preview／Cancel 不变；Apply 仅修改 `reference_cells.value`；地址、两侧哨兵、calls 计数不变；显式恢复 | 未执行 |
| RV-H02 | core1 重复 RV-H01；Scope All 下当前操作不广播；切核再回来后原草稿 not_sent，peer 私有对象不变 | 未执行 |
| RV-H03 | 右值引用、结构体引用成员及引用到结构体：填写实际 path，独立读取 referent 地址和值；不能把 reference 绑定槽当作写入目标 | 未执行 |
| RV-H04 | frame 1 的 `local_reference`，probe=`local`：正确修改局部对象；切帧原草稿过期；const／volatile reference 在 Preview 拒绝，无赋值 | 未执行 |
| RV-H05 | shared referent：全部关联核心暂停、实际 shared owner；仅一次写入，另一个核心独立读取共享结果；运行中 peer 拒绝 | 未执行 |
| RV-H06 | 实际标量类型为 128 位的目标：不同高低字、unsigned 最大值、signed 最小／最大值／-1；逐字节独立核对、相邻哨兵不变；越界输入拒绝 | 未执行 |
| RV-H07 | 实际 GDB 截断／不支持 128 位常量：Preview 或 Apply 前的位核对拒绝，无赋值；超时／发送后错误不重试或回写旧值 | 未执行 |

```powershell
# 默认准备，4 skipped，不访问目标。
node scripts/test-variable-write-hardware.cjs

# 环境就绪并填写模板后执行；当前任务不运行上板命令。
node scripts/test-variable-write-hardware.cjs --run --binary <debugtui.exe> `
  --project <dedicated-project.toml> --core core0 `
  --case <filled-variable-reference.json> --fixture-function debug_reference_fixture
```

驱动先核对实际类型、referent 的独立地址／大小和原始值可恢复性；取消和草稿重用均不发送赋值。实际写入 verified 后才显式恢复；任何前置条件或写后验证失败立即停止，不自动回滚。保留完整事件、工具身份、工程 SHA、独立读回、owner 和恢复结果。

只有 `reference_fixture_has_int128=1` 且实际 ELF 含 16 字节可赋值标量时才执行 RV-H06。32 位 Cortex-R52 编译器没有 `__int128` 时，此项保持 skipped／not applicable，不把结构体、两个 64 位成员或 Q 向量伪装成同一种变量 writer。CPU 向量写入仍需独立适配。位域仍未接入：实测 GDB `sizeof` 返回声明类型宽度，6 位 signed 字段 hexadecimal 会以 32 位符号扩展；必须另外验证 DWARF bitpos／bitsize、容器 RAM 范围、新鲜邻接位及真实 typed assignment，不能只删除当前拒绝。
