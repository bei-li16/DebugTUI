# DebugTUI 功能测试

用例保存在 DebugTUI 仓库中：`src/**/tests*.rs` 和模块内 Rust 测试、`tests/` 中的夹具/集成测试，以及 `scripts/test-*.cjs` / `scripts/test-*.ps1` 中的功能验收驱动。生成的配置、测试程序、屏幕文本、MI 日志和结果放在被 Git 忽略的 `artifacts/`，不进入安装包。

## 统一运行

Windows 完整验收需要 Rust 工具链、Node.js、npm、PowerShell 7 (`pwsh` 在 PATH)、本机 GCC 和 GDB。终端用例需要支持 ConPTY 的 Windows 10 1809 或更新版本。测试不依赖额外 Node 包。

```powershell
./scripts/build.ps1
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe `
  --gdb C:/MinGW/bin/gdb.exe --cc C:/MinGW/bin/gcc.exe
```

可以把 `--binary` 指向已安装的 release EXE，核对实际交付程序。Rust 测试仍针对当前源码运行；报告记录被测 EXE 的路径和 SHA256，不能将两个来源混为一谈。CLI 和 npm 测试要求 EXE 版本与仓库 `package.json` 版本一致。

默认顺序运行 **17 个套件**：Rust 单元/集成测试、CLI、真实终端、隔离 npm 分发、退出、Pause、环境配置、本机 GDB、补全、变量树、搜索、断点、内存、SVD、日志、多核协调、关联断点。`--gdb` / `--cc` 也可通过 `DEBUGTUI_TEST_GDB` / `DEBUGTUI_TEST_CC` 指定。缺少必需工具、未通过断言或超时会保留原因，完整验收不会把未执行项计为通过。

```powershell
# 局部复测；不表示其余功能通过
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe --only cli,terminal
```

每次运行生成独立的 `artifacts/functional-*/report.json`、`report.md` 和逐套件日志。JSON 报告关联子套件的详细结果；新增用例有稳定 ID、通过/失败/跳过状态，失败返回非零退出码。硬件复位/下载默认跳过，在详细报告中显示。

## 功能覆盖

[`functional-coverage.json`](functional-coverage.json) 将软件划分为 **22 个功能组**，映射到实现文件、必需套件、Rust 测试证据及验证限制：

| ID | 功能 |
|---|---|
| F01–F03 | CLI/JSONL、配置与路径兼容、Setup/Projects/Examples |
| F04–F07 | 服务/连接/重连、执行控制、代码断点、数据观察点 |
| F08–F12 | Watch/进制、结构体/数组/指针树、补全、源码选择与标签、Files/Symbols |
| F13–F16 | 栈/Locals/寄存器、反汇编/内存、SVD/外设、运行时读取与刷新 |
| F17–F19 | 多核与关联断点、Build/Download 任务生命周期、日志 |
| F20–F22 | 外观/动效/键鼠/窗口尺寸、退出与清理、安装/升级/卸载/重装 |

`automated-passed` 表示该组所列套件和单元测试证据通过，**不是代码行/分支覆盖率，也不是全部硬件和输入组合都已验收**。多核套件使用多个真实本机 GDB，不能替代多核实板；demo 终端验证实际输入和渲染，不能替代目标执行。物理拔插/断电、多小时稳定性、其他宿主/探针、系统剪贴板、颜色/字体及公共 Release 下载另行验收。具体限制保存在矩阵和每次报告中。

## 本轮新增用例：45 项

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

`tests/conpty.cs` 只用于测试，解析本轮 Ratatui/ConPTY 输出所需的光标和清除指令，保留完整 VT 输出供复核；它不是完整终端模拟器，不对颜色/字体做断言。测试使用自身启动的终端，避免操作用户正在使用的窗口或系统剪贴板。npm 测试使用 `artifacts/` 下的私有前缀，不替换全局已安装软件。新增脚本只清理自己启动的调试进程。
