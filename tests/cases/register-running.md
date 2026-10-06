# 运行态寄存器读取：延后环境用例

本 Goal 不执行上板，以下全部 **SKIPPED**。软件 worker/MI/Tcl 与实际 EXE 驱动验证协议、门禁和生命周期，不等于板卡 AP 在运行态必然可读。

执行前记录型号/revision、探针、固件、GDB/OpenOCD 版本、各核/AP/target/端口、目录来源；独立期望值来自手册/固件或另一可靠调试工具，不从 DebugTUI 返回值反填。固件应稳定运行，普通控制配置在测试区间保持不变。准备恢复步骤和日志目录，不自动使能模块、改权限或执行维护命令。

| ID | 前置条件与步骤 | 预期与恢复 |
| --- | --- | --- |
| REG-RUN-H01-ROUTE | M3/M4/M7 当前核停止；检查 CorePrivate 的独占核 AP、target 与显式 `while_running=true` 通道。 | 每核目录和路线正确；实际通道不可用时先修配置，不回退 GDB。 |
| REG-RUN-H02-AP | 通过 GDB 继续选定核，实际 RUNNING；单次读取独立基线中的 CPUID、CCR，M7 可补 cache/TCM 普通配置。 | 值匹配，`running_memory`，owner/endpoint/target/channel、当前 context、响应区间齐全；无停止帧 MI 查询或 AP 写入。 |
| REG-RUN-H03-HALT | 运行态手动读取 r0、R52 CP15/MPU/selector；M7 读取 CCSIDR。对停止通道、GDB-only、丢失/错核通道分别复测。 | NeedHalt 或具体路线错误；数据 I/O 为零，不自动 halt、不换后端。 |
| REG-RUN-H04-STALE | 运行态成功读后暂停，再运行、重连或换路由；记录旧样本。 | 旧值保持原时间/来源且 stale；新请求成功才更新。测试结束暂停选定核并关闭会话。 |
| REG-RUN-H05-SIDE-EFFECT | 在 TUI 打开含 DHCSR/SysTick CTRL 的列表，多次空闲重绘；根据固件独立状态显式手动单次读。 | 自动队列/Probe 无这些地址请求；手动各一次。可能读清状态，记录副作用，不用它们检测 run/halt。 |
| REG-RUN-H06-RACE | 注入受控通信延迟；读期间暂停/继续、取消、断线；延迟需由测试环境实现。 | 不接受迟到值，不继续剩余 batch，不用 0 代替失败，不自动重试；恢复通信后重新显式读取。 |
| REG-RUN-H07-OWNER | M7＋M4 或 R52 多核分别使用相同地址/不同路由并切核、改 owner；按每核独立基线核对。 | 无串值或跨核来源；安全 AP 路线可运行态读，借核路线仍 NeedHalt。完整异构验收另由 B10 覆盖。 |

可执行驱动覆盖 H01～H04（M3/M4/M7）：

```powershell
node scripts/test-register-running-hardware.cjs
# 默认不连接、不运行目标，四项 SKIPPED。
# 用户在环境就绪后执行：
node scripts/test-register-running-hardware.cjs --run --binary <debugtui.exe> --project <debug.toml> --core <核名> --case <独立期望JSON>
```

基于 `tests/fixtures/register-running-board.example.json` 填写真实预期和路线；删除 `software_example` 或设为 false。`--software-fixture` 仅用于无板软件验收，不用于绕过板级期望。驱动会通过 GDB 继续/暂停选定核，失败后尝试暂停并关闭本测试会话；不修改工程文件。H05～H07 记录人工/故障注入步骤，不声称四阶段驱动已经执行它们。

核心参考绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
