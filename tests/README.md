# DebugTUI 功能测试

用例保存在 DebugTUI 仓库中：`src/**/tests*.rs` 和模块内 Rust 测试、`tests/` 中的夹具/集成测试，以及 `scripts/test-*.cjs` / `scripts/test-*.ps1` 中的功能验收驱动。生成的配置、测试程序、屏幕文本、MI 日志和结果放在被 Git 忽略的 `artifacts/`，不进入安装包。

## 统一运行

Timer 单项一致性新增 `timer_samples_keep_extreme_u64_values_native_reads_and_legacy_evidence` 和 `timer_counter_driver_validates_carry_wrap_freeze_regression_and_sample_windows`。后者以实际 EXE 分别运行全 64 位回绕/低字进位、允许冻结，以及冻结必须进展/倒退/异常高字/过旧基线四种失败流程；固件变量由独立软件夹具供给，不算上板。原有传输故障测试同时核对请求完成时间只在收到完整帧后出现，旧来源 JSON 不制造终点。生产 C 另验证 48 个动态 pair 回读，源与候选二进制沿用已固定构建；详见 [一致性边界](../docs/register-timer.md)。

专用 Timer 后端的软件验证见 [Timer 自检](../docs/register-timer.md)。`cargo test --locked timer_` 覆盖独立协议、编码/位宽、完整物理证据、权限未知与拒绝、选定核、旧值来源、恢复未知、取消和读取期间实际线程/帧变化。REG-H05 驱动以实际 EXE 分别执行旧 MRRC 和新 Timer 双核软件流程；case 的 `require_timer_adapter=true` 要求并记录各次 Timer 的 MIDR/EDSCR/DSPSR/DLR，同时保留独立固件基线比较。`node scripts/test-register-timer-hardware.cjs` 默认五项 SKIPPED，无目标访问；[十四项环境 case](cases/register-timer.md) 均未执行。生产 C 事务及 Windows/Linux 候选构建另见 [后端说明](../tools/openocd-adapter/README.md)，软件模型不代表芯片验证。

共享寄存器归属验证见 [REG-109 自检](../docs/register-shared-owners.md)。`cargo test --locked --test register_shared` 的五项四核 MI 管道集成覆盖三种 scope、两个 cluster、未知归属、alias 单次读取、peer 生命周期、迟到值、失败及重连、首个通知中的兼容投影；两项模型、一项故障边界和一项 UI 测试另核对身份边界、旧 JSON、逐路由缓存、失败退避及宽窄键盘帮助。F24 本批增加九个证据模式，当前累计 69 个。[八项人工 case](cases/register-shared-owners.md) 全部 SKIPPED。

缓存生命周期验证见 [REG-106 自检](../docs/register-cache-lifecycle.md)。`cargo test --locked --test register_lifecycle` 覆盖真实 worker／MI 的换帧、别名、复位成功及部分失败、Console 名称索引变化、更换 ELF、双核缓存及重连；App 测试另外验证返回原帧和迟到回复。F24 在该历史批次加入七个证据模式，当时累计 60 个。[十项人工 case](cases/register-cache-lifecycle.md) 均为 SKIPPED，未执行上板测试。

寄存器基础框架自检见 [逐项审计](../docs/register-framework-audit.md)。新补强的目录读取边界、大小端与 128 位别名、固定列／字段访问覆盖、逐核字段变化高亮及全枚举帮助由 `src/registers/tests.rs`、`tests/register_access.rs`、`src/ui/registers/framework_tests.rs` 和 status tests 验证。F24 在该历史批次映射 53 个证据模式；[延后环境 case](cases/register-framework.md) 均未执行，实际终端视觉仍单独计 REG-208。

寄存器状态分类与取消的软件证据包含 `src/ui/registers/status/tests.rs`、`tests/register_cancel.rs`、`tests/selector_access/cancel_cases.rs`。后两者使用实际 worker／MI 管道及真实 Tcl 控制流，覆盖最后一项丢弃、已知恢复／未知结果、MPU／VFP 和 Scope All；不连接板卡。人工环境 case 见 [状态与取消](cases/register-status-cancel.md)，全部未执行。

寄存器显示／偏好回归包含 Rust 纯值与 Ratatui 测试，以及 `tests/register_display.rs` 调用的 `scripts/test-register-display.cjs`。后者启动两个实际 DebugTUI 进程，并发合并不同核心的配置，检查旧全局设置不会覆盖新视图、无效请求不写文件和无调试器访问；不需要 Python 或板卡。可单独运行 `node scripts/test-register-display.cjs --binary target/debug/debugtui.exe`。

Windows 完整验收需要 Rust 工具链、Node.js、npm、PowerShell 7 (`pwsh` 在 PATH)、本机 GCC 和 GDB。终端用例需要支持 ConPTY 的 Windows 10 1809 或更新版本。测试不依赖额外 Node 包。

选择器的软件集成测试另需带标准 `tkinter` 的 Python：`python -c "import tkinter; print(tkinter.Tcl().eval('info patchlevel'))"` 应返回 Tcl 版本，测试不会创建 GUI 或连接板卡。可用 `DEBUGTUI_TEST_PYTHON` 指定实际 Python EXE。`node scripts/test-register-selectors-hardware.cjs` 默认生成 skipped；只有显式 `--run --project FILE --core NAME --case JSON --binary FILE` 才连接所声明的暂停夹具。示例值需要换成实际固件证据，具体前提见 [选择器说明](../USER_GUIDE.md#读取-mpupmu-选择器组开发分支) 和 [验证记录](../TESTING.md)。

```powershell
./scripts/build.ps1
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe `
  --gdb C:/MinGW/bin/gdb.exe --cc C:/MinGW/bin/gcc.exe
```

可以把 `--binary` 指向已安装的 release EXE，核对实际交付程序。Rust 测试仍针对当前源码运行；报告记录被测 EXE 的路径和 SHA256，不能将两个来源混为一谈。CLI 和 npm 测试要求 EXE 版本与仓库 `package.json` 版本一致。

默认顺序运行 **20 个套件**：Rust 单元/集成测试、CLI、真实终端、芯片选择 TUI、芯片选择原生 GDB、源码重映射、隔离 npm 分发、退出、Pause、环境配置、本机 GDB、补全、变量树、搜索、断点、内存、SVD、日志、多核协调、关联断点。`--gdb` / `--cc` 也可通过 `DEBUGTUI_TEST_GDB` / `DEBUGTUI_TEST_CC` 指定。缺少必需工具、未通过断言或超时会保留原因，完整验收不会把未执行项计为通过。

```powershell
# 局部复测；不表示其余功能通过
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe --only cli,terminal
```

每次运行生成独立的 `artifacts/functional-*/report.json`、`report.md` 和逐套件日志。JSON 报告关联子套件的详细结果；新增用例有稳定 ID、通过/失败/跳过状态，失败返回非零退出码。硬件复位/下载默认跳过，在详细报告中显示。

## 功能覆盖

[`functional-coverage.json`](functional-coverage.json) 将软件划分为 **26 个功能组**，映射到实现文件、必需套件、Rust 测试证据及验证限制：

| ID | 功能 |
|---|---|
| F01–F03 | CLI/JSONL、配置与路径兼容、Setup/Projects/Examples |
| F04–F07 | 服务/连接/重连、执行控制、代码断点、数据观察点 |
| F08–F12 | Watch/进制、结构体/数组/指针树、补全、源码选择与标签、Files/Symbols |
| F13–F16 | 栈/Locals/寄存器、反汇编/内存、SVD/外设、运行时读取与刷新 |
| F17–F19 | 多核与关联断点、Build/Download 任务生命周期、日志 |
| F20–F22 | 外观/动效/键鼠/窗口尺寸、退出与清理、安装/升级/卸载/重装 |
| F23 | 芯片目录、Setup 新增及核心选择 |
| F24–F25 | 开发分支的寄存器目录／按需视图、Setup 与各面板的显式内存通道 |
| F26 | 开发分支的写入规划、SVD 写元数据和 Core 的预览／应用／取消 |

`automated-passed` 表示该组所列套件和单元测试证据通过，**不是代码行/分支覆盖率，也不是全部硬件和输入组合都已验收**。多核套件使用多个真实本机 GDB，不能替代多核实板；demo 终端验证实际输入和渲染，不能替代目标执行。物理拔插/断电、多小时稳定性、其他宿主/探针、系统剪贴板、颜色/字体及公共 Release 下载另行验收。具体限制保存在矩阵和每次报告中。

## 开发分支寄存器与内存夹具

Timer 的 `node scripts/test-register-timer-hardware.cjs` 默认五项 skipped；显式运行需 `--run --project FILE --core NAME --case JSON --binary FILE`。以 `tests/fixtures/register-timer-board.example.json` 为起点，在专用 Hyp 固件 ready 循环暂停，替换四个稳定 CVAL/offset 期待值和每核独立参考符号，并记录计数器 debug freeze 与允许时间窗。`register-timer-board.c` 先检查正常执行 Hyp，再只读采样全部九项 MRC、六项 MRRC；TVAL 是动态项，关闭时 TVAL/ISTATUS 为 UNKNOWN。实际 EXE 双核软件基线六阶段通过，SVC/未就绪/参考值不同在正确阶段停止并清理。GNU Arm 11.4 离线编译及全部十五项指令编码核对通过，没有目标代码执行。正常 EL 权限与 Debug state 的 EDSCR.HDD/Hyp debug 授权分开验收，不能把该 Hyp 基线流程当作通用权限适配；[详细范围](../docs/register-timer.md)、[十二项环境 case](cases/register-timer.md) 明确未执行项。

单核 case 去掉 `peer_core` 和 `control_scope`，使用实际核心名。固件参考值必须为每核独立存储；ready 前的 DMB 只保证存储排序，不清理缓存，实际 GDB/CPU/AP 对参考 RAM 的可见性和缓存行为须独立核验。

银行读取使用`registers::banked::tests`与`tests/selector_access/banked_cases.rs`：三十项名称×八模式的成功/拒绝、当前EL与停止前DSPSR反例、逐核Scope All/上下文/取消、旧协议/裸值/伪造/截短/权限拒绝及旧值证明。生产C验证37成功、41受限、162个EL1 Unknown、1110个故障点与完整DSPSR/DLR、R0物理回读；独立GNU汇编核对三十条银行指令。直接当前CPSR在Debug态受约束不可预测，后端不注入该指令，EL1具体模式保持未知。Windows/Linux与本机离线命令无板卡I/O。

延后REG-H02驱动`node scripts/test-register-banked-hardware.cjs`默认4 skipped、不连接。显式`--run --project FILE --core NAME --case JSON --binary FILE`要求独立当前Debug EL、停止前mode、MIDR、peer和每核固件参考。正常固件`register-banked-board.S`有三十个只读槽，八模式离线编译；User无MRS/MRC。真实EXE的软件模型验证Hyp、User七项当前银行/二十三项拒绝、EL1 Unknown三种五阶段流程，board_tests_executed=false。硬件边界及[八项case](cases/register-banked-proof.md)均SKIPPED；驱动禁止从停止CPSR推断当前Debug EL，不发送模式/控制写入。

MPU／MAIR 使用 `registers::mpu::tests`、`ui::registers::mpu::tests` 和 `tests/selector_access/mpu_cases.rs`。软件 TCP 夹具提前启动 Python，以 JSONL 复用解释器执行真实 Tcl；没有扩大生产连接超时。延后驱动 `node scripts/test-mpu-regions-hardware.cjs` 默认 4 skipped，实际运行需 `--run --project FILE --core NAME --case JSON --binary FILE`，见 `tests/fixtures/mpu-regions-board.example.json` 与只读固件钩子 `mpu-regions-board.c`。板级期望必须独立填写所有区域与 MAIR；软件示例只能配合 `--software-fixture`。

`tests/register_access.rs` 在 Windows 下启动隔离的 Node MI 夹具，验证目录模式连接不自动扫描寄存器、稀疏索引、逐项失败、精确 64 位值和异步 RUNNING 结果失效；同时检查连续内存块、64 位地址和 Scope All 下只读取选中核心。`src/session/memory.rs` 的 TCP 夹具检查显式 target／endpoint、地址序字节及无效请求在连接前拒绝；UI 单元测试检查各档布局、取消不保存、策略隔离和迟到响应丢弃。

```powershell
cargo test --locked
cargo test --locked --test register_access
cargo clippy --locked --all-targets -- -D warnings
```

这些夹具不启动 OpenOCD、不连接板卡，不证明芯片的寄存器实现、AP 映射或缓存一致性。完整任务的后端扩展、写入和延后上板用例仍在开发，见 [开发进度](../docs/registers-development-status.md)。

## Setup 源码映射专项（0.8.7）

`scripts/test-source-remap.ps1` 覆盖 POSIX、Windows 反斜杠、混用、空格四种服务器路径，实际运行 ConPTY Setup 的扫描、预览、选择、保存、重启和开关操作；随后由 GDB 验证本地源码读取及绝对 file:line 断点解析，POSIX 样例再检查双核各自的映射继承。另有取消、超时、无调试信息、GDB 错误和扫描中退出五个清理场景。鼠标列表/Apply/Cancel 命中与层级操作由 Rust `launch::remap::tests` 覆盖。

```powershell
./scripts/test-source-remap.ps1 -Binary ./target/release/debugtui.exe
# 使用真实工程 ELF，但不连接板卡；不修改该工程配置
./scripts/test-source-remap.ps1 -Binary ./target/release/debugtui.exe `
  -Gdb /path/to/arm-none-eabi-gdb.exe -Elf /path/to/firmware.elf -SourceRoot /path/to/local/source
```

UNC 目录边界及别名由 Rust 测试覆盖；GDB 对不可达网络主机的路径解析可能超时，不将其声称为已通过的端到端场景。用例使用隔离副本；不执行 profile 的参数或初始化、不启动服务、不烧录或运行目标。

## 2026-09-27 新增用例：45 项

开发分支的 `tests/write_access.rs` 还覆盖声明 RAM 范围的边界／哨兵、64 位地址、4096 字节、非零帧、GDB 写权限与结果状态，以及真实 TCP TCL MMIO 的物理 CPU 暂停、大小端、保留位、RO、W1C、WO、读副作用及单字访问。共享 owner 只接受协调器内部核心状态，不接受 JSON peer 声明；All scope 不广播。

`node scripts/test-memory-write-hardware.cjs` 默认只准备四个 skipped 用例，不连接目标。运行专用 RAM 夹具示例：`node scripts/test-memory-write-hardware.cjs --run --binary target/release/debugtui.exe --project PATH --core core0 --fixture-function ram_write_fixture --address 0x20000004 --bytes 12345678`；AP 路由另加 `--channel ap`。工程需声明正确区域及暂停相关核心。成功写入后独立读取范围和两侧哨兵，再显式恢复；不自动运行／停核／复位，也不在未知或不符结果后回滚。`--software-fixture` 保留软件层标识，不能作为上板证据。

以下计数是带稳定 ID 的验收场景数，不是 JSONL 请求数；其中硬件 18 项包含两个默认不执行的选项。既有 Rust 和 GDB 专项测试继续由统一入口调用，不重复计入新增数。

| 文件 | ID | 数量 | 检查 |
|---|---|---:|---|
| `scripts/test-cli.cjs` | CLI-01–12 | 12 | 真实进程版本/帮助/错误参数、UTF-8 路径、脚本协议、失败终止/清理、配置拒绝 |
| `scripts/test-terminal.ps1` | TERM-01–08 | 8 | 真实 ConPTY：Symbols 跳转、Files 输入焦点/清空、Setup/Projects/Examples、Appearance、缩放、退出 |
| `scripts/test-distribution.ps1` | PKG-01–07 | 7 | 本地包边界、隔离前缀升级、入口、哈希、配置保留、重复安装、卸载/重装 |
| `scripts/test-project-hardware.cjs` | HW-01–18 | 18 | STM32F429 FreeRTOS 实板调试，详见下表 |

Files 的正向筛选/键鼠打开由既有 `ui::search::tests` 和 `test-search-gdb.cjs` 覆盖；demo 没有真实源文件列表，TERM-03 只检查输入与焦点。PKG-02 使用旧版本号夹具和同一 EXE，检查包替换机制；历史正式版迁移和公共下载使用 `scripts/test-release.ps1` 单独验证。

## STM32F429 / FreeRTOS 实板

```powershell
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe --hardware `
  --elf G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/build/FreeRTOS_STM32F429.elf `
  --profile G:/Data/GitFiles/DebugTUI/tools/debug-env-cmsis-dap.toml `
  --source-root G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project `
  --project G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/debug.toml
```

`--hardware` 增加 **3 个套件**：新工程验收、既有 STM32 代码/数据断点专项、既有运行时内存专项，共 20 个套件。此命令会实际连接、暂停、运行和单步开发板，使用隔离配置启动自有 OpenOCD。运行前结束占用 3333/6666 的已有调试会话；新工程用例发现端口占用会拒绝启动，不结束其他会话。工程 ELF 必须已在板上；校验失败后新工程用例跳过执行/求值场景。

可用 `--svd` 指定 SVD，默认 STM32F429。这里的 `--project` 仅作为原始工程文件的哈希保护对象，不把原始工程作为运行配置；运行配置由 ELF/profile/source-root 参数生成。硬件专用脚本另支持 `--break-location`、`--function`，默认周期任务 `BSP/USER_TASK/Src/user_task.c:41` / `Task100ms`。Watch 依赖 `xTickCount`、`Log_Tx_En`、`g_w25q_jedec_id`，不适用于任意固件。

| ID | 检查 |
|---|---|
| HW-01–02 | 真正连接并获得暂停帧；只读段与 ELF 一致 |
| HW-03–04 | 标量/静态 Watch、无效表达式、函数/变量/文件搜索且 PC 不变 |
| HW-05–07 | 代码断点编辑/保存、周期任务临时断点、源码/指令单步 |
| HW-08–09 | 栈帧/Locals/寄存器/反汇编；指针/结构体/数组展开与删除 |
| HW-10–11 | GDB/AHB/暂停核心读取一致；安全外设读取和通道/位宽/对齐错误 |
| HW-12 | 运行时 AHB 计数变化，零额外 MI 轮询、不隐式暂停 |
| HW-13–14 | 重连、重复 Connect 拒绝、重启后的 Watch 恢复 |
| HW-15 | `--allow-reset`：真实复位/Run，在 main 临时断点暂停 |
| HW-16 | `--allow-reset --allow-download`：真正 GDB 烧录并再次校验 |
| HW-17–18 | 正常退出、自有进程消失；原工程/profile/SVD/ELF 哈希不变 |

默认不编译或烧录应用。`--allow-reset` 会复位板卡；`--allow-download` 会烧写给定 ELF，并要求同时指定 `--allow-reset`。Build/Download 的外部命令、双管道输出、失败、超时、取消和会话恢复，由既有 `tests/project_tasks.rs` 与多核套件覆盖；这不能证明用户工程的实际编译工具链成功。

## 测试实现边界

开发分支写入软件用例使用 `cargo test --locked`：`writes::tests` 覆盖精度、非连续字段、枚举／约束、特殊位“不动作”值与未知规则拒绝；`ui::writes::tests` 覆盖真实 Ratatui 布局、键鼠、预览／取消、过期及错误保留输入；`tests/write_access.rs` 使用真实 worker 和严格 MI 子进程，覆盖稀疏索引、权限变化、线程／帧／核心、重连、发送后错误、超时／断连及准确结果状态。它们都不访问板卡。

延后执行的 Core 写入实板用例已准备在 `scripts/test-register-write-hardware.cjs`。默认运行只生成四项 skipped 报告，没有连接或写入：

```powershell
node scripts/test-register-write-hardware.cjs
# 环境允许时，对专用测试固件在已暂停的测试函数中运行；不在本任务中执行。
node scripts/test-register-write-hardware.cjs --run --binary <EXE> --project <专用工程.toml> `
  --core core0 --register r0 --fixture-function debugtui_write_test_halt
```

分别准备 MCAL／Bao 的 core0、core1、双核工程，在工作寄存器可恢复的专用测试停止点执行；驱动要求相关核心已经暂停，不自动 Pause／Run／Reset／Download。用例检查取消无写、单次应用、独立 GDB 读取、邻接寄存器不变、重复草稿不重放，以及成功后明确恢复普通工作寄存器。失败或结果未知时不会盲目恢复旧值。保留实际 EXE／工程 SHA256、owner、掩码、原始 MI 事件和逐项结果；本次仅用 `--software-fixture` 在隔离 MI 夹具运行过，报告明确 `board_tests_executed=false`。这只覆盖 WRITE-H02 的 Core 子集，不能代替其他 WRITE-H 或系统／外设 writer 实板用例。

`tests/conpty.cs` 只用于测试，解析本轮 Ratatui/ConPTY 输出所需的光标和清除指令，保留完整 VT 输出供复核；它不是完整终端模拟器，不对颜色/字体做断言。测试使用自身启动的终端，避免操作用户正在使用的窗口或系统剪贴板。npm 测试使用 `artifacts/` 下的私有前缀，不替换全局已安装软件。新增脚本只清理自己启动的调试进程。

## 芯片目录与核心选择

- `scripts/test-devices.ps1`：ConPTY 验证 core1 选择/保存/重开、双核、新增 s32k144、升级保留自定义目录。隔离 `DEBUGTUI_CONFIG_DIR`，不改用户目录。
- `node scripts/test-devices-gdb.cjs target/release/debugtui.exe`：真实本机 GDB，10 种芯片/核心组合，逐核身份、Run/Continue/Pause、偏好保存恢复。不是目标芯片实板测试。
- `node scripts/test-devices-tha6206.cjs target/release/debugtui.exe <MCAL工程根> --allow-reset`：THA6206 实板 core0/core1/双核，使用 `debug-chip.toml` 和 `.vscode/debug-env-chip.toml`。会 chipreset，不 Build/Download；先检查端口空闲，生成隔离项目，原配置哈希保持不变。
- 安装套件启用真实 postinstall，验证用户目录初始化和客户条目在升级时保留。

Bao 配置适配回归：`pwsh -NoProfile -File scripts/test-bao-setup.ps1 -ProjectRoot <Bao工程根>` 验证真实项目的 Setup；`node scripts/test-bao-devices.cjs <EXE> <Bao工程根> --verify-only` 只检查板上 Flash 与 Bao smoke 镜像一致性，`--allow-reset` 才在一致性通过后运行 core0/core1/双核。驱动不会下载固件，测试均使用隔离项目文件。

Bao Build/Download 回归：`node scripts/test-bao-tasks.cjs <Bao工程根>` 使用隔离副本测试缺失清单、ELF 被更换、WSL 失败后的旧产物失效、原始错误编码及下载器非零退出码，不访问实板。

`node scripts/test-bao-workflow.cjs <EXE> <Bao工程根> --flash 0` 验证 core0；末尾改为 `0,1` 验证双核。该驱动会重新构建并烧录 Bao smoke，要求开始时板上已是匹配的 smoke 固件；覆盖连接状态下 Build/Download、释放探针、重连和符号加载、全镜像 Flash 回读、C 断点/单步、Guest 心跳。它使用隔离 TOML 并检查原始项目/profile 哈希不变。

VFP 使用 `registers::vfp::tests`、能力事实单元测试和 `tests/selector_access/vfp_cases.rs`，覆盖 86 个存储／控制视图、共享 pair、D16 不授权 Q、协议／精确宽度／身份／权限、未使能、未知原始 MVFR、物理线程／帧前后检查、未知故障隔离和 Scope All 当前 owner。C 测试编译实际生产头文件，核对 63 个故障点、R0/R1 恢复回读和前后 DSPSR／HCPTR／FPEXC 一致。`tools/openocd-adapter/tests/vfp-encoding.s` 用 GNU Arm 汇编器独立核对 VMRS、VMOV 与 HCPTR 编码。REG-H02/H03 用显式 `unsigned int *` 读取汇编定义的参考数组／ready，避免依赖缺失的调试类型；真实 native GCC/GDB 验证无类型符号的高位原始字，记录 `artifacts/vfp-untyped-reference-gdb.log`。

REG-H03 驱动 `node scripts/test-register-vfp-hardware.cjs` 默认 4 skipped，不访问目标。显式参数为 `--run --project FILE --core NAME --case JSON --binary FILE`；以 `tests/fixtures/register-vfp-board.example.json` 为起点，替换实际固件／CPU／MVFR／使能／陷阱证据并移除 `software_example`。启动固件建立 Hyp 权限和 FP 内容，主机驱动不运行、下载、复位或使能；工程设置 `on_exit="disconnect"`，禁用连接时控制动作，只在 `register-vfp-board.S` 的 ready 循环暂停。参考数组与 ready 变量须每核独立，ready 命名为 `${reference}_ready`；可用 `DEBUGTUI_VFP_REFERENCE`、`DEBUGTUI_VFP_READY`、`DEBUGTUI_VFP_HOOK` 编译独立钩子。钩子先检查 Hyp/HCPTR，TCP10=1 不注入 VMRS，EN=0 不注入 FPSCR／VMOV，D16 不读高 D，无模式或 FPU 写入。

编译示例：`arm-linux-gnueabi-gcc -mcpu=cortex-r52 -mfpu=neon-fp-armv8 -mfloat-abi=softfp -c tests/fixtures/register-vfp-board.S`。实际二进制的双核软件用例分别验证 D32、SP-only D16、EN=0、TCP10=1 四类五阶段流程；独立参考低／高字对照所有合法 S/D/Q 原始位及控制值，记录 peer、PC/CPSR/HCPTR、R0/R1 和 FPEXC／工程未变。GDB scratch 样本只是逻辑视图，物理回读由适配器执行；报告 `board_tests_executed=false`。合法 EL1/Guest 读取仍待补，不能用安全拒绝代替完整 REG-H03 验收。

VFP 原始写入后端单独验证：`tools/openocd-adapter/tests/vfp-write-transfer.c` 编译生产头文件，覆盖 80 个 S/D/Q 视图、144 个故障点、Q 部分写入、NaN payload/符号高位、相邻位及物理控制保护；GNU Arm 独立核对写入 VMOV 编码。`python tools/openocd-adapter/tests/vfp-write-driver.py --out artifacts/vfp-write-driver` 运行 5 项真实 TCP 双核夹具测试，覆盖 S/D/Q 明确恢复、pending cache 拒绝、unknown/mismatch 后不继续或重试。默认 `python tools/openocd-adapter/tests/vfp-write-hardware.py` 生成 4 skipped，不连接。显式运行模板为 `tests/fixtures/register-vfp-write-board.example.json`；八类硬件 case、独立初始固件基线与外部 PC/GPR/写后样本限制见 [VFP 写入验收](cases/register-vfp-writes.md)。

DebugTUI writer 软件验证见 `registers::vfp::writes`、目录和配置校验、`ui::registers` 键鼠／窄终端测试及 `tests/selector_access/vfp_write_cases.rs`。实际 worker＋真实 Tcl/TCP 覆盖原始 S/D/Q 与新鲜相邻位、明确容量／权限／协议、草稿取消和重复／别名失效、实际 GDB frame 变化、Scope All 单 owner、128 位 BE 字节输入、伪造回执与 unknown 隔离、mismatch 和写后上下文变化。主机延后驱动 `node scripts/test-register-vfp-write-hardware.cjs` 默认 4 skipped；显式参数 `--run --project FILE --core NAME --case JSON --binary FILE`，模板为 `tests/fixtures/register-vfp-host-write-board.example.json`。实际 DebugTUI 二进制的 S/D/Q 正常流程各 5 阶段，unknown/mismatch 负向流程核对不恢复／重试；初始值使用独立固件字数组，后续别名由单独 reader 读取。仍不表示完整 WRITE-005/008–012 验收或上板通过。

PMU 验证使用实际生产 C 事务和 worker/TCP/MI/Tcl 软件模型，区分当前 Debug EL2 与停止前 CPSR，验证直接索引保持选择器及完整周期位宽。`scripts/test-register-pmu-hardware.cjs` 默认不连接目标，显式运行要求独立固件基线、已暂停且计数已关闭的专用夹具；不就绪/已启用/参考矛盾均拒绝。十项延后环境 case 见 [PMU](cases/register-pmu.md)，实际证据及限制见 [说明](../docs/register-pmu.md)。

GIC：`selector_access/gic_cases.rs` 通过实际 worker/TCP/MI/Tcl 检查 29 个读入口、ICC/ICH/ICV AP 过滤、双核 owner、旧值证明、取消/上下文变化、协议/权限/伪造与不确定故障；生产 C 固定编码模型与真实 OpenOCD dummy 命令单独验证。`node scripts/test-register-gic-hardware.cjs` 默认五项 SKIPPED；[十项环境 case](cases/register-gic.md)、每核只读 GNU Arm 固件 hook 和 JSON 示例已准备，不把软件夹具当上板。物理 MMIO、低 EL ICV 及完整 Debug 模块仍待完成。

R52 MMIO：register_mmio.rs 验证四个非连续 worker/两 cluster、显式 owner/base、缺失映射无回退，以及真实 TCP AP 的字序/地址/target/来源和错误旧值。另通过实际 DebugTUI EXE 执行二十四项独立 RAM 驱动，正常流程及未就绪/错误 owner/身份/参考高字拒绝均正常清理。node scripts/test-register-mmio-hardware.cjs 默认五阶段 SKIPPED；[十二项环境 case](cases/register-mmio.md)、只读 GNU Arm 固件和 JSON 模板已准备。[自检](../docs/register-mmio.md) 明确配置容量与非原子总线对；不把软件夹具或离线对象当作上板通过。

新鲜 MMIO Probe：四项独立手册数据单元验证身份、容量、请求/owner/context/AP来源、拒绝字段和旧配置兼容；五项真实worker/EXE测试覆盖42请求、普通读保持Proof、peer在请求中及发布后使共享事实失效、失败旧值保留、big-endian AP两字与独立RAM驱动正/负向流程。入口 node scripts/test-register-mmio-probe-hardware.cjs 默认五项 SKIPPED；[六项环境case](cases/register-mmio-probe.md)、独立21项C/JSON仅准备及离线编译。[范围与证据](../docs/register-mmio-probe.md) 不代表完整REG-405/BUS-006或上板完成。

S/D/Q视图证据见[自检](../docs/register-storage-views.md)，包含同次128位pair/容量raw、独立64字/80视图及四种正常startup对象；[八项环境case](cases/register-storage-views.md)均SKIPPED。`DEBUGTUI_TEST_ARTIFACT_ROOT`可把Node测试产物放到其他磁盘；功能入口同时收集工程/该目录/独立分发目录的本轮报告，默认仍用工程artifacts。Rust/Tcl夹具仍保留工程内的命令日志，不改变用例或断言。
