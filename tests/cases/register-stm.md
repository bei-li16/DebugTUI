# REG-406 延后环境用例

本批不执行上板。以下全部 SKIPPED；软件夹具通过不能改变该状态。先确认芯片确有 STM-500、控制区/可选接口及 AP 路线，保留板级地址依据、工具版本、固件 ELF 和 capture 来源；各核心使用自己的独立 RAM 基线。

| Case | 预备操作和验收要求 | 状态 |
|---|---|---|
| STM-H01 | 确认 CoreSight/STM 身份和 STM-500 part；停止 frame 0；Probe 与原始请求、owner、路由匹配，再与六项 RAM 基线比较 | SKIPPED |
| STM-H02 | 芯片没有已确认 STM 或缺映射时移除映射；Probe 与手工读均不得访问猜测地址，无归属 fallback 不可使用 | SKIPPED |
| STM-H03 | 分别配置已确认的 HWE/DMA；验证类/revision、事件容量、mux/trigger 条件；没有接口时不探测可选地址 | SKIPPED |
| STM-H04 | 使用真实供电/认证/AP 拒绝场景；仅受影响类失败，不自动解锁、不切换路线；失败刷新保留旧值原证据 | SKIPPED |
| STM-H05 | 四核逐核独立 capture、Probe 和读取；Scope All 仍只读取所选 worker，核标记、endpoint/target 与 owner 一致 | SKIPPED |
| STM-H06 | 对端在 Probe 中及之后运行/停止；chip 代次失效，旧值标记 stale，新数据不得靠旧 Probe 读取；其他操作保持可用 | SKIPPED |
| STM-H07 | 静态端口/事件 bank；读取前后 SPSCR/SPMSCR/HEBSR 不变；mask 仅解释当前 bank；不得隐式迭代选择器 | SKIPPED |
| STM-H08 | GDB/AP 字节序及权限分别验证；WO、stimulus、保留地址零访问；项目与控制状态不变，不采集 Trace | SKIPPED |

默认只产生延后报告，不创建 GDB 会话：

```powershell
node scripts/test-register-stm-hardware.cjs
```

环境具备时，先在固件流程中集成 `tests/fixtures/register-stm-board.c`，传入板级确认的控制映射与实际 core index，完成 capture 后停在静态断点。它只读 STM 并将六项基线写入 RAM，不启用 Trace、不修改选择器。不要用 GDB evaluate 调用 capture 函数。独立确认该固件/板级映射允许这些访问。

复制 `tests/fixtures/register-stm-board.example.json` 到实际环境文件，替换每项原始值、RAM 符号、核心标记、frame 函数、owner、base 和实际 route，设 `software_example=false`，再运行：

```powershell
node scripts/test-register-stm-hardware.cjs --run --binary target/debug/debugtui.exe --project path/to/project.toml --core cpu.0 --case path/to/stm-board.json
```

该只读驱动执行五阶段：停止/就绪检查、组件 Proof、六项原始值与 RAM 比较、配置/选择器不变检查、断开。H02-H08 的负向和可选场景依照表中步骤分别准备并留存实际请求记录；驱动的默认子集不代表它们全部执行。软件验证可明确加 `--software-fixture`，报告 `board_tests_executed=false`。
