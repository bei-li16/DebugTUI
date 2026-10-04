# WRITE-H01 特殊 float／double 延后用例

状态：**SKIPPED／未执行上板测试**。软件 MI 模型和本机 GDB 结果不表示 R52 板卡已通过。

使用 `tests/fixtures/variable-write-board.c` 的 `float_fixture32`／`float_fixture64`，带调试信息、`-O0`，不把对象声明成 const／volatile。把固件停在 `debug_write_fixture`，驱动不执行 run、halt 或 reset。工程预先加入相应 Watch 并声明实际普通 RAM；确认每个相关核心均暂停。目标函数调用哨兵 `variable_write_fixture_calls` 必须纳入独立检查。

选择 `variable-float32-board.example.json` 或 `variable-float64-board.example.json`，填写实际 core owner、shared scope、RAM、frame、path 和目标字节序。Bytes 输入自己的 `little_endian` 用于解析输入，JSON 顶层的字节序必须对应真实目标内存。shared RAM 另声明 chip／cluster owner，并检查关联核心；不要复制 core-private owner 作为共享内存证明。

| Case | 操作与预期 | 当前状态 |
|---|---|---|
| VF-H01 | core0 私有 float：正负 Infinity、两个 quiet 和两个 signaling payload；Preview／Cancel 不变，Apply 一次，独立字节读取一致，邻接与 calls 不变 | 未执行 |
| VF-H02 | core1 私有 double：相同六种模式，保留高 32 位、符号和 payload；peer 私有对象不变 | 未执行 |
| VF-H03 | 双核 Scope All 分别选择 core0／core1 重复以上流程；同一操作只作用于当前 owner | 未执行 |
| VF-H04 | shared RAM：所有关联核心暂停，使用实际 chip／cluster owner；读回与两侧哨兵一致，另一个核心可以独立读取共享对象 | 未执行 |
| VF-H05 | 非零调用帧的局部 float／double：用 Locals 路径与独立地址检查；切帧或切核后原草稿拒绝 | 未执行 |
| VF-H06 | 无 Python ARM GDB：正负 Infinity 若常量位核对通过可赋值，需要 buffer 的 payload 在 Preview 拒绝，RAM／calls 不变 | 未执行 |
| VF-H07 | 每阶段保留完整事件、工具版本／SHA、原工程 SHA、独立读取及 owner；仅所有测试赋值 verified 后显式恢复原始 Bytes 并独立验证 | 未执行 |

```powershell
# 准备报告：不连接板卡，5 skipped。
node scripts/test-variable-write-hardware.cjs --special-floats

# 环境就绪后填写参数，当前任务不执行此命令。
node scripts/test-variable-write-hardware.cjs --run --special-floats `
  --binary <debugtui.exe> --project <dedicated-project.toml> --core core0 `
  --case <filled-variable-float32.json> --fixture-function debug_write_fixture
```

驱动先证明原始值可以恢复，再检查取消、一次写入、六种特殊值、独立内存、邻接和显式恢复。任一前置条件、赋值或验证失败即停止，不自动回滚或重试；草稿应用第二次必须 not_sent。无 Python 的完整 payload 阶段不能标为 passed。VF-H03／04／05 需要按对应工程和 case 分别执行；不能用单核运行替代矩阵。
