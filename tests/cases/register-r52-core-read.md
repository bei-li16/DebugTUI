# R52 常用只读访问的延后硬件 case

本 Goal 不执行上板。本表硬件状态全部 **SKIPPED**，软件模型或实际 EXE 驱动通过不改变这个状态。

参考文档：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

权限细节另按本地 `ARM DDI 0568A.c` 的 F1.3.4/G2.1.8 复核。使用专用正常固件和客户项目副本；记录真实 R52 revision、探针、AP/target/endpoint、固件、GDB、OpenOCD 与 EXE 版本/摘要。实际后端必须先完成当前补丁的构建与命令入口验收，旧候选不能替代。期望值由独立固件或 ADS 捕获，不复制本次 DebugTUI 输出。

| ID | 操作与独立预期 | 状态 |
| --- | --- | --- |
| R52-CORE-H01-ROUTE | 停止在 physical frame 0，显式选择核并 Probe。MIDR/MPUIR/HMPUIR 保留新协议来源及当前 Debug 证明；保存的 User DSPSR 不误判为当前 EL0。不改变模式取得权限 | SKIPPED |
| R52-CORE-H02-VALUES | 按独立 baseline 读取 15 项常用标量及容量内直接 region，核对原始位、字段和各 bank 容量。带 RES1[16/18] 的合法 EDSCR 不误拒绝。另以实际低 EL/受限环境验证拒绝原因，不执行改变权限的步骤 | SKIPPED |
| R52-CORE-H03-PEER | 对两核使用不同 target/AP。Scope All 下请求仍只访问所选核；显式切核后同一编码返回该核独立值，旧 context 拒绝。共享 endpoint 不等于共享私有值 | SKIPPED |
| R52-CORE-H04-PRESERVATION | 前后独立核对 R0、PC、DSPSR/DLR、控制与 selector；直接读取不写 selector/配置。取消、链路失败或恢复未知不得发布新值、自动重试；失败旧值保留原来源。未知恢复保留隔离诊断 | SKIPPED |

默认检查零目标 I/O：

```powershell
node scripts/test-register-r52-core-hardware.cjs
```

将 `tests/fixtures/register-r52-core-board.example.json` 复制到环境专用位置，替换 software_example、工具信息、AP/target/endpoint、独立 MIDR/DSPSR 和各核 values。未实现的 region 从独立 baseline 清单移除；无第二核时保留 peer case SKIPPED。不要把示例常量当真实板级预期。

环境就绪后显式执行：

```powershell
node scripts/test-register-r52-core-hardware.cjs --run --binary C:\Test\debugtui.exe --project C:\Test\r52-project.toml --core core0 --case C:\Test\r52-core-board.json
```

驱动不执行 reset、continue、模式或控制写入；保存 project/EXE/case 摘要、完整 JSONL 和阶段报告，核对错误 baseline 必须失败。`--software-fixture` 只用于软件模型验证，报告 `board_tests_executed=false`。AP 编号和工具字符串是声明信息，不代表驱动探测到了真实 AP。低 EL/断线/取消的实际操作需在专用环境另按表执行并记录，不用普通成功驱动报告替代。

退出时关闭本次专用会话，项目副本保持原字节。发生恢复未知时停止注入，保存 FAULT/隔离证据，按专用板级流程恢复后重连；不得通过重试读取猜测恢复状态。
