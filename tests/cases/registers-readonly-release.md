# 本版只读系统寄存器用例与执行入口

本清单对应冻结 Goal 的 A～D 功能和 E01 交付。所有实板用例当前 **SKIPPED**；本 Goal 不执行上板。已有软件证据保留在 [验收账本](../../docs/registers-readonly-goal.md)，这里关联入口、独立基线和人工步骤，不把测试数量当 feature 数。

## 软件验收层次

| 验收对象 | 实际软件入口 | 能证明 / 不能证明 |
| --- | --- | --- |
| 配置、每核 CPU/目录、继承、来源 | `register_configuration`、`register_inheritance`；实际 EXE 的 configuration/matrix/display 脚本 | 生产解析与返回；不证明目标型号或 AP 映射 |
| M ID、CorePrivate、副作用、运行态 | `register_policy`；`selector_access` 的 M/多核/运行态 case | 实际 worker/MI/TCP/Tcl 分派及零 I/O 拒绝；返回字节是软件夹具 |
| M MPU / M7 cache | `selector_access/m_profile_mpu_cases.rs`、`m_cache_cases.rs` | 真实 Tcl 控制流、selector 恢复及错误/取消边界；不证明 ARM 总线行为 |
| R52 core / MPU / selector | `selector_access/r52_core_cases.rs`、`r52_mpu_cases.rs`、`r52_selector_cases.rs`；实际 EXE 驱动 | 当前证明、成功/受限/故障、独立值比较与 owner/context；不证明 ARM 注入已上板 |
| R52 生产事务与 OpenOCD 入口 | 固定 adapter 的 `r52-transfer.c` / `r52-selector-transfer.c`；Windows/Linux 实际命令测试 | 编译生产 C、独立物理 I/O 模型、native command handler；未执行目标 ARM 指令 |
| UI、状态、生命周期和空闲资源 | Ratatui/status、`register_access`、`register_cancel`、`register_lifecycle`、`register_shared`；真实 Windows TUI 测量 | 显示、旧值、核/帧/运行/重连、按需请求及短期 CPU/内存；不保证长期无泄漏 |
| 本版模板与延后入口 | `node scripts/test-register-readonly-readiness.cjs --binary <EXE绝对路径>` | 实际 EXE 离线加载两份模板；八个驱动默认 SKIPPED 且禁止子进程/网络连接 |

Rust 使用 `cargo test --locked --test <套件>`；专项可加测试名过滤。Tcl 集成需带 tkinter 的 Python，可用 `DEBUGTUI_TEST_PYTHON` 指定。发布前 E03 运行完整回归和静态检查；文档/模板交付不重复无关全量测试或重建未改后端。

## 延后硬件入口

以下八个脚本**无参数运行**产生 report，全部 SKIPPED，`board_tests_executed=false`，不启动 DebugTUI/GDB/OpenOCD、不连目标。全 SKIPPED 报告的 `passed=false` 是正确结果。

| 功能 / case 文件 | 可执行驱动（`scripts/`） | 独立基线模板（`tests/fixtures/`） |
| --- | --- | --- |
| [M Debug/DWT/FPB/FPU](register-cortex-m-modules.md) | `test-m-profile-modules-hardware.cjs` | `m-profile-modules-board.example.json` |
| [M MPU](register-cortex-m-mpu.md) | `test-m-profile-mpu-hardware.cjs` | `m-profile-mpu-board.example.json` |
| [M7 cache/TCM](register-cortex-m7-cache.md) | `test-m7-cache-hardware.cjs` | `m7-cache-board.example.json` |
| [M7＋M4](register-cortex-m-multicore.md) | `test-m-profile-multicore-hardware.cjs` | `m-profile-multicore-board.example.json` |
| [M 运行态](register-running.md) | `test-register-running-hardware.cjs` | `register-running-board.example.json` |
| [R52 core](register-r52-core-read.md) | `test-register-r52-core-hardware.cjs` | `register-r52-core-board.example.json` |
| [R52 MPU 总览](register-r52-mpu-read.md) | `test-mpu-regions-hardware.cjs` | `r52-native-mpu-board.example.json` |
| [R52 MPU selector](register-r52-selector-read.md) | `test-register-selectors-hardware.cjs` | `r52-native-selector-board.example.json` |

测试人员在另一次明确安排上板的任务中，把模板复制为真实 case，填写目标/revision、探针及 AP/target/每核 GDB/Tcl 端口、工具版本/路径/哈希、独立固件 ELF/停止点、参考值来源和恢复方法。改 `software_example=false` 前须独立复核，不能复制本次 DebugTUI 响应作为 expected。`--software-fixture` 只用于软件模型，不转为 board verified。

单核类共同形式（本 Goal 不执行）：

```powershell
node scripts/test-register-r52-core-hardware.cjs --run --binary C:\Cases\debugtui.exe --project C:\Cases\debug.toml --core core0 --case C:\Cases\r52-core-board.json
```

替换脚本及对应 JSON 执行其它单核类。多核驱动从 case 的 `cores` 声明选择核心，**不传 `--core`**：

```powershell
node scripts/test-m-profile-multicore-hardware.cjs --run --binary C:\Cases\debugtui.exe --project C:\Cases\debug.toml --case C:\Cases\m7-m4-board.json
```

保存配置/EXE/基线 SHA256、独立参考、JSONL、MI/Tcl、工具摘要和 report。暂停流程要求实际物理 frame 0；运行态驱动在独立固件中由 GDB resume 并清理，不能用于客户工作负载。DHCSR 的一次 sticky 读取须 case 显式允许，其它自动队列仍不得读 DHCSR/SysTick CTRL。

成功须核对全位值、owner/context/target/endpoint、当前 Debug 与停止前证据、前后控制值及 selector/scratch 恢复。缺少环境/可选 peer 为 SKIPPED；权限、协议、容量、独立值、恢复或上下文断言不符为 FAIL，不降格 SKIPPED。恢复不确定立即停止，保存 FAULT/隔离，按板级步骤处理，不盲目重试、改权限或自动换后端。

## 人工补充与恢复

驱动覆盖正常读取和记录检查；不会自动制造异常、切 Guest/低 EL、改变授权或破坏恢复。下列人工步骤仍为准备好的 SKIPPED case：

- 公共 [configuration](register-configuration.md)、[structured policy](register-structured-policy.md)、[M catalogue](register-cortex-m-catalogues.md)、[M ID/NVIC](register-cortex-m-probe.md)：来源/优先级冲突、未知身份/容量、副作用和错路由。M ID 没有独立硬件驱动，按 case 使用既有 Probe/JSON 接口。
- 多核 H05～H08、运行态 H05～H07：cache 错核、生命周期、失败隔离、身份变更与访问能力撤销，保留两核独立日志。
- R52 core/MPU/selector 及 [Debug state](register-r52-debug-state.md) 中的低 EL/HDD、容量漂移、取消/故障和恢复未知：由独立固件/调试环境准备状态，不让 DebugTUI 自动改变授权。
- [framework](register-framework.md)、[status/cancel](register-status-cancel.md)、[cache lifecycle](register-cache-lifecycle.md)、[shared owner](register-shared-owners.md)：树/字段/枚举/格式、旧值来源、核/帧/运行/暂停/重连及 owner epoch；查看缓存零请求。

正常结束移除本次 Watch/测试断点，核对原 selector、scratch、控制值和项目文件，按隔离工程退出策略断开。需要 reset/重刷的异常固件由测试人员按 case 操作；disconnect 不等于硬件复位或保证停核。实板验证前源码锁/分发声明的 board verified 保持 false。

核心参考（绝对路径不变）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
