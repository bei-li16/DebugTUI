# DebugTUI 验证记录

## 0.8.6 发布前验证（2026-09-23）

- 168 项 Rust 单元测试和 1 项 CLI 集成测试通过，2 项显式外部环境测试默认忽略；严格 Clippy `--locked --all-targets -- -D warnings`、Release 构建和 `git diff --check` 通过。
- 新 Release EXE SHA256 为 `A3C9F1609B19C5081D3118FD68F1542027730B9F500098C414BABAA15FB488E9`，与下述单核/多核实板回归使用的已安装 0.8.6 完全相同，无需将其他构建的实板结果冒用为本次结果。
- `test-exit-policy.cjs` 的 26 个场景通过，覆盖 remote/extended-remote/local、各退出策略、运行/暂停/READY，以及 continue/release/exit 失败、GDB hang/crash。`test-search-gdb.cjs` 的真实本机 GDB 和 MCAL ARM ELF 两组测试通过，确认符号位置、有界查询、不改变目标执行状态。
- README 和用户手册同步为 0.8.6，更新工程专用 Setup、当前 Examples、Files/Symbols 搜索、工具/工程配置职责以及本轮 MCAL 实板结果和残留限制。XeLaTeX 三轮编译无缺字和未解析引用；PDF 共 83 页。
- 日志：`artifacts/release-0.8.6/`。全仓 `cargo fmt --check` 仍报告 HEAD 已有的 `src/svd.rs` 排版差异；该文件未被本次修改，不计为格式检查通过。

## 0.8.6：THA6206 单核/多核实板回归（2026-09-23）

- 直接测试已全局安装的 0.8.6，EXE SHA256：`A3C9F1609B19C5081D3118FD68F1542027730B9F500098C414BABAA15FB488E9`。工程为 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440`，使用实际 GHS ELF、ARM GDB、OpenOCD 和 THA6206；UI 通过真实 Windows ConPTY 输入/读取验证。
- **16 个功能组通过**，包括 core0/core1 独立调试、默认单 GDB 模式、双核 Scope/联停/复位、条件和多核断点、逐核 8 个硬件代码断点及第 9 个拒绝后恢复、运行中 Live Watch、Files/Symbols 跳转、源码映射、偏好保存、任务失败恢复和外部附加。退出/重连另有 **21 个实板组合场景**全部通过。两核联停由软件协调，不是 CTI 同时停机；本轮 peer STOPPED 日志延迟样本为 124–266 ms。
- 初始板上 Flash 与本地 ELF 不匹配，已通过工程 Load.bat 下载现有配套 MCAL HEX，随后及最终分别校验向量/代码段匹配。本次实际烧写了固件，但没有重新编译应用、修改用户源码或替换已安装软件；构建成功生命周期使用 echo，另行验证真实下载流程及非零返回码恢复。
- 修正两类工程工具配置问题并复测：默认环境补充板级 Run 动作，避免 `-exec-run` 触发不支持的通用 reset；默认和外部 core1 环境补充 17 个通用寄存器白名单，避免复位入口自动读 VFP 引发 DSCR.ERR。只改工程 `.vscode/debug-env.toml` 和 `.vscode/debug-env-core1.toml`，附原因注释；19 个受监测文件中的其余 17 个哈希未变，四份工程 TOML 保持原样。
- **残留问题保留记录**：首次调试复位仍可报 `Failed to write memory at 0x00000000`，后续 examine/Run/Flash 校验成功但不证明复位写入确认；GHS 深层栈存在 `pc 0x80`、未知帧及栈回溯时的调试访问错误。当前帧/断点/单步测试通过，不将其解释为完整调用栈可靠或日志零错误。
- 最终两核暂停在 `0x08000000`，按 detach 结束，自有 DebugTUI/GDB/OpenOCD 和 3333/3334/6666/4444 监听清理。未覆盖物理断电/拔插、多小时压力、浮点调试和其他固件；未提交、push、升版或安装。

详细结果、配置修正、原始失败分类及验证边界：[`artifacts/regression-0.8.6-20260923/report.md`](artifacts/regression-0.8.6-20260923/report.md)。机器可读汇总：[`report.json`](artifacts/regression-0.8.6-20260923/report.json)。

## 0.8.5 工作区：Setup 仅编辑工程配置（2026-09-23）

- 移除 GDB executable、GDB arguments、Target mode、Endpoint、Start service、Timeout 六个工具编辑项。Tools / profile 仅选择环境引用；On exit、ELF、源码、任务、SVD、日志及保存开关保留。标题提示当前核心名称。F4 模板不再生成内嵌工具参数；旧配置覆盖值继续兼容。
- 168 项单元测试、1 项 CLI 集成测试通过，2 项外部环境测试仍忽略；严格 Clippy 和 Release 构建通过。新增覆盖普通单核、core0 单核、core1 单核、双核编辑/保存、切换 profile、逐核运行时偏好合并、工具文件不改写、遗留工具值保留和工具字段从界面移除。
- 新 EXE 的真实 Windows ConPTY 验证：MCAL debug-core0/debug-core1/debug-multi，以及 Bao debug/debug-dual 共 5 份工程副本，点击 On exit 后 Ctrl+S 保存，只改变工程退出策略，其余配置完全一致。原始工程、工具 profile、已安装 EXE 的哈希均未变化。 Bao 的 debug-dual.toml 当前没有 [[cores]]，属于双核固件、单个 GDB 会话的配置；此次保持该连接模式，TUI 多会话覆盖由 MCAL debug-multi 和独立双 GDB 用例验证。截图和结果：`artifacts/project-setup-20260923/setup-*.png`、`native-setup-results.json`。
- `node scripts/test-exit-policy.cjs target/release/debugtui.exe` 的 26 组退出流程回归通过。另用隔离 profile 验证普通单核、core0、core1、双核 × detach/resume/disconnect，共 12 组 MI 回归通过；Scope=core 时退出仍释放所有配置核心。证据：同目录 `profile-core-exit-results.json`、`*.mi.txt`。最初测试暴露严格 fixture 缺少多核连接后的 register-cache flush 响应，已补充该精确命令；终端脚本也改为等待完整表单绘制，避免读到半帧。这些是测试驱动问题，没有修改实际退出执行逻辑。
- 本轮未连接板卡，未安装、未升版。本地 Release SHA256：`EDC7A875ABDF09DCB8DDB5957E410CBFE1390F432C73B1B0DCFB77AD61ED5515`；已安装 0.8.5 仍为 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。详细日志位于 `artifacts/project-setup-20260923/`。

## 0.8.5 工作区：工程与工具配置职责说明（2026-09-23）

- 修正 README 和 Setup 帮助：工程配置保存程序、构建/下载、源码映射、Watch/断点、核心选择及退出策略；工具环境保存 GDB/OpenOCD、服务启动、连接默认值及通信超时。现有 `[session].timeout_ms` 是工具通信超时；`on_exit` 属于工程退出策略。明确区分解析器的继承兼容能力与建议的字段归属。
- 本轮只修改说明，未更改解析、优先级或保存行为，未迁移 MCAL/Bao 配置。Setup 修改工具项仍会写项目覆盖；README 已明确此限制。
- 15 项启动配置测试通过，Release 构建通过，未安装。日志：`artifacts/config-layering-20260923-{tests,build}.txt`。构建产物 SHA256：`F862820B93A1E40B967A7A41C3500499D9BE7AEB94B8379CDC559D957FDA120C`。

## 0.8.5 工作区：输入区可见性与配置说明（2026-09-23）

- Symbols、Files 和 Watch 使用独立背景及完整边框，聚焦时强化边框/底色，Watch 的 Add 按钮有独立边界；高度不足时保留紧凑单行。Watch 面板为边框预留空间，保留原有可见变量容量。Setup 的 profile 说明明确继承值与项目覆盖，并修正远程 resume 的过时 detach 描述。
- 165 项单元测试、1 项 CLI 集成测试和严格 Clippy 通过，2 项外部环境测试仍忽略；Release 编译通过。既有布局、点击区域、Watch 删除/添加及自动补全测试覆盖 45×12 至 180×50。日志和实际 Ratatui 颜色预览：`artifacts/input-visibility-20260923/`。
- 新 Release EXE 在真实 ConPTY 中加载 Bao 的 `bin/tha6xxx/tha6206-smoke/bao.elf`：点击 Symbols 边框，输入 `uartclock` 并跳到 `tha6xxx_uart.c:12`；点击 Files 边框输入 `boot` 并打开 `boot.S`；点击 Watch 边框输入表达式、核对光标位于框内、清空并点击 Add。三组通过。使用隔离 TOML 和 local 模式，仅加载 ELF；没有连接探针或运行目标。验证时仅对测试子进程移除工具环境中的 `NO_COLOR=1`，以核对正常彩色终端效果。
- 本轮**未安装、未升版**。新 `target/release/debugtui.exe` 的 SHA256 为 `0D482825B17D0636871C817F681F929E27B2A6286DD76D2636F8E5A43FF8B54C`；已安装 0.8.5 保持 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。完整终端结果和画面：`terminal-result.json`、`files-focused.png`、`symbols-focused.png`、`watch-focused.png`。

## 0.8.5：已安装版本 Files / Symbols 搜索验收（2026-09-23）

确认全局 `debugtui --version` 为 **0.8.5**，实际 EXE 为 `C:/Users/01766/AppData/Roaming/npm/node_modules/@debugtui/cli/bin/debugtui.exe`，SHA256 与本地 Release 一致：`0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。本轮直接运行该已安装 EXE，没有重新编译或安装。

- **16 个功能组通过**。基于 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440` 的实际 GHS ELF、工程 ARM GDB 16.3，通过 Windows ConPTY 输入真实鼠标/键盘事件并读取终端显示；双核部分实际连接 THA6206 和工程 OpenOCD，非 mock / demo，也不是人工 VS Code 鼠标验收。
- **Files**：从 ELF 读取 563 个源文件；输入 `sPi` 同时显示 `Spi.c`、`Spi_Irq.c`、`espi_hal.c`、`espi_std.c`。筛选后键盘分别打开三个文件、鼠标打开 `espi_hal.c`，核对 Source 标签；无匹配时 Enter 不误打开，Ctrl+U 恢复 563/563 列表。
- **Symbols**：键盘查询 `Spi_Init` 并定位 `Spi.c:4073`；鼠标查询 `uartcnt` 模糊匹配 `uart_cnt` 并定位 `Uart_Demo.c:1519`；类型 `Spi_ConfigType` 定位 `Spi_GeneralTypes.h:591`；静态变量 `SpiExternalDevice_ConfigParamCore0` 定位 `Spi_PBcfg.c:481`。逐项核对真实源码定义及 Source 选中行。无匹配、正则字符按字面处理、200 项/类别上限提示通过。
- **映射与布局**：使用隔离源码副本及 `source_map`，跳转后实际显示映射副本标记且仍为第 1519 行。80×24、45×12 的独立终端均显示结果文件/行号并正确跳转。
- **双核实板**：Core0 / Core1 分别查询并跳转 `Spi_Init`，各核日志均记录实际符号查询；跳转前后两核 PC、当前栈帧标题保持不变。两核运行时新查询显示等待提示，OpenOCD 同时确认 `running running`；显式 F6 后查询完成并跳转第 1519 行，两核为 `halted halted`。已有查询结果可在两核继续运行时用于源码跳转，不隐式停核。
- 测试使用隔离 TOML；已安装 EXE、ELF/HEX、四份工程 TOML、profile 和 OpenOCD 配置共 9 个文件前后哈希一致，没有编译/烧写固件。结束后核对调试进程和 3333/3334/6666 监听清理。早期测试驱动的路径分隔符、空结果提示文案和连接完成等待条件已纠正；原始失败记录保留，最终双核结果以 `hardware-results.json` 为准，未发现此次搜索功能缺陷。

汇总及证据索引：[`artifacts/search-installed-0.8.5-20260923/report.json`](artifacts/search-installed-0.8.5-20260923/report.json)。终端画面、输入序列和 GDB 日志位于同目录；独立 ELF 元数据验证为 `artifacts/search-arm-elf-1790135230928/result.json`。

## 0.8.5：THA6206 退出修复与实板复测（2026-09-23）

修复 0.8.4 的 `session.on_exit="resume"`：远程 all-stop 会话改为清理断点后 `-exec-continue` → `-target-disconnect`，不再对运行中的目标发送被 GDB 拒绝的 detach。本地进程仍在暂停状态下 detach。`-gdb-exit` 的错误、非零退出码和退出超时会报告失败；收到 `^exit` 后最多等待 3 秒，让 GDB 正常清理，再使用强制回收作为兜底。

- **已安装 0.8.5 并验证全局命令**；全局 EXE 与被测 Release EXE 的 SHA256 均为 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。已有工作区内的 Source 文本操作、启动配置及 Files/Symbols 搜索改动也包含在本地构建中；本轮未提交、push 或发布远端 Release。
- **安装后 21/21 实板退出与重连场景通过**：Core0、Core1、双核 × detach/resume/disconnect × STOPPED/RUNNING 的 18 项，以及三种核心配置的 `remote` 模式 resume 3 项。用独立 OpenOCD 在断开后观察实际核状态、AHB 计数器，再重连验证源码行、Watch 和禁用断点恢复。单核 resume 仅对应核运行，双核 resume 两核运行；detach/disconnect 在此工程保持暂停。双核 resume 的计数器示例为 1270→1372、1952→2054。不能将这些目标状态推定到其他 Server。
- **常规实板回归通过**：Core0 的源码/寄存器/栈、单步、条件/忽略次数/临时硬件断点、Watch 结构体/数组/指针、三类硬件数据断点、内存和外设读数、运行时 AHB、重连/复位；Core1 独立调试时 Core0 计数器仍增长；双核控制 27 项检查、跨核断点 61 次请求、两核运行时 Live Watch 5 组采样。Flash 向量与代码段再次 `compare-sections` matched。旧回归脚本的 `number="all"` 和 `mode` 参数错误已在隔离测试副本中改为 `all=true`、`access`，断点专项复测通过，不能把该脚本错误计为产品缺陷。
- **真实 ConPTY 界面 4/4 组通过**：源码双击选择 `uart_cnt`、Ctrl+C 系统剪贴板复制、Add to Watch；两核运行时右侧 Watch 实际显示 LIVE 且数值 1165→1246→1407→1488→1650；Pause/F2/Esc 保留会话；运行中 Ctrl+Q 按 resume 正常结束自有调试进程。测试等待复制完成的界面提示后读取剪贴板，结束后恢复原剪贴板。
- **自动化与构建**：165 项单元测试、1 项 CLI 集成测试通过，2 项依赖外部环境的单元测试保持忽略；严格 Clippy、Release 构建通过。新增 `node scripts/test-exit-policy.cjs` 的 26 项生命周期测试通过，覆盖本地/远程、未 Run、停止/运行状态、真实错误传播、延迟退出、退出挂起/异常码；严格 fixture 在旧 0.8.4 上复现原来的运行中 detach 错误。Pause 的 6 种事件/超时场景、本机多核 GDB 13 组、Files/Symbols 的本机与 THA ELF 离线查询也通过。完整 `cargo fmt --check` 仍指出原有 `src/svd.rs` 格式差异，本轮未改动该文件。
- **本地安装包**：生成 0.8.5 EXE/ZIP/TGZ，验证校验和、无捆绑工具、隔离 npm 升级/卸载/重装、PowerShell/CMD 入口和渲染。TeX 用户手册已同步退出说明并成功重新生成 PDF。

记录集中于 [`artifacts/exit-policy-fix-20260923/`](artifacts/exit-policy-fix-20260923/)；安装后矩阵为 `installed/matrix.json`，常规回归为 `functional/`，最终终端记录为 `terminal-hardware-1790133832/`。本轮使用 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440` 的既有 GHS ELF/HEX，没有重新烧写固件；Build/Download 的新增复测只验证连接生命周期，使用输出标记的测试命令，真实编译/烧写结果见下方 0.8.4 完整回归记录。未在本轮重测 STM32 或 Bao 实板。

额外发现的范围外限制：本机 MinGW GDB 16.2 对 Windows 测试进程执行 detach 后再退出 GDB，进程的文件心跳停止；直接运行同一 GDB/MI、完全绕过 DebugTUI 也复现（`native_direct.py` / `native/direct-mi.log`）。因此这里只确认本地 detach 命令被接受，不宣称该后端的进程在 GDB 退出后持续运行；没有用假通过掩盖此现象。该本机进程问题不影响上述 ARM GDB 16.3 + OpenOCD 的 THA6206 实板结果。

## 0.8.4：Files 与 Symbols 模糊搜索（2026-09-22 验证，09-23 收尾）

- 最终代码通过 **165 项单元测试、1 项 CLI 集成测试**；2 项依赖实板或系统剪贴板的测试保持忽略。严格 Clippy 与 Release 编译通过。日志：`artifacts/search-ux-20260922/{unit-tests,clippy,build}.txt`。本次只编译，未安装或替换全局 EXE。
- UI 回归覆盖 `spi` 对四类文件名的大小写及模糊匹配、筛选后键盘/鼠标打开正确文件、空结果、清空与粘贴焦点隔离；符号查询的防抖、单个在途请求、输入变化/切核后的旧响应丢弃、运行状态门控、错误重试，以及 CI 路径映射后跳转行号且保持当前栈帧不变。45×12、80×24、120×36 的实际 Ratatui 文本预览位于同一目录，长路径保留文件名和行号。
- 真实本机 GDB 离线查询通过：函数、全局/静态变量、类型、模糊字符顺序、正则特殊字符按字面输入、无匹配和结果上限。`Spi_Init` 与 `SpiCounter` 分别返回测试源码第 6、3 行；25 条查询命令均为符号/文件元数据。记录：`artifacts/search-native-1790070992527/result.json`。
- 工程内 ARM GDB 加载 GHS 生成的 `THA6206_Demo_Prj.elf` 离线查询通过：`Spi_AsyncCheckJobLinkStatus` 定位到 `Spi.c:1882`，`SpiExternalDevice_ConfigParamCore0` 定位到 `Spi_PBcfg.c:481`，已核对本地定义行。12 条查询命令均为元数据查询。记录：`artifacts/search-arm-elf-1790070991205/result.json`。两组测试始终为 READY，没有连接探针或执行目标程序；这不是 THA6206 单核/多核实板运行验证。
- 可用 `node scripts/test-search-gdb.cjs target/release/debugtui.exe` 重跑本机验证；设置 `DEBUGTUI_TEST_GDB` 为工程 GDB 路径，并在 EXE 参数后追加 ELF 路径，可重跑对应 ELF 的离线验证。上述真实 GDB 验证后仅修改搜索 UI 的长路径显示及粘贴焦点保护，最终代码已重新执行全部单元测试、Clippy 和构建。
- 最终产物：`target/release/debugtui.exe`，版本 `0.8.4`，3,658,240 字节，SHA256：`97BF95AA283355A381229C4016BFD8D4A782A06F757389AE21BC9E7A42CA413B`。

## 0.8.4：未通过项的专项复测与原因（2026-09-22）

上轮的唯一失败功能组是 **`session.on_exit="resume"`：结束调试前恢复目标运行**。针对同一个已安装 0.8.4 EXE 重新执行了 16 个专项场景：9 个对照/恢复场景通过，7 个 resume 场景稳定复现失败。这里的场景数是对一个失败功能组的展开，不是新增 7 类缺陷。记录：[`artifacts/exit-policy-retest-20260922/report.json`](artifacts/exit-policy-retest-20260922/report.json)。

| 策略与场景 | 本轮复测 | 退出后的实际观测 |
| --- | --- | --- |
| `detach`：Core0 / Core1 / 双核，各自从 STOPPED 和 RUNNING 退出 | 6/6 通过 | DISCONNECTED；被调试核暂停。本适配中默认退出不会自动保持运行。 |
| `resume`：Core0 / Core1 / 双核，各自从 STOPPED 和 RUNNING 退出 | 6/6 失败 | 返回 FAULT，但相应核实际仍在运行；Core0 或双核运行时 AHB 计数器持续增长。 |
| `disconnect`：Core0 / 双核，从 RUNNING 退出 | 2/2 通过 | DISCONNECTED；退出前暂停。 |
| DebugTUI 自己启动 OpenOCD 的原始使用方式，Core0 `resume` | 再次失败 | 同一 GDB 错误，排除独立观察用 OpenOCD 的服务归属差异。 |
| 失败后重新连接，读取 Watch，再用默认 `detach` 退出 | 通过 | 能恢复连接和读取变量，正常清理退出。 |

原因定位到 `src/session.rs` 的 `Engine::disconnect`：它先完成暂停/断点清理，再在 resume 分支发送 `-exec-continue`，收到 GDB `^running` 后仍发送 `-target-detach`。当前 GDB 16.3 在这个运行状态拒绝 detach：`Cannot execute this command while the target is running`。随后 `-gdb-exit` 关闭 GDB，DebugTUI 把前述错误汇总为 FAULT。应用内的 DISCONNECTING 标签不会改变 GDB/硬件的实际运行状态。

本轮保留一个由测试持有的独立 OpenOCD，在每次断开后检查两个核的真实状态及 AHB 数据：单核 resume 只留下相应核运行，双核 resume 留下两个核运行。例如双核暂停状态退出后，`uart_cnt` 为 872→977；从运行状态退出后为 1223→1325。故此次失败是退出顺序和结果处理错误，Continue 本身已成功。不能仅凭 FAULT 标签判断板卡已停止。

当前工程三个用户配置均从 `.vscode/debug-env.toml` 继承 `on_exit="detach"`，失败策略只在隔离测试副本中启用。后续修复需要为后端实现合适的脱离/恢复顺序，并验证退出后硬件状态；不能通过忽略 detach 错误伪造通过。现有 `tests/mock-gdb.cjs` 对 detach/disconnect 不区分运行状态而直接返回 done，缺少此次实板拒绝条件，应补相应回归。

本轮没有修改退出实现或升版；没有重新编译/烧录固件。已安装 EXE、ELF/HEX、三个用户配置的前后哈希一致；测试结束无调试进程和 3333/3334/6666 监听残留。

## 0.8.4：已安装版本 THA6206 实板功能回归（2026-09-22）

**结论：27 个功能组通过，1 个退出策略缺陷复现，不能判定为全部通过。** 本次运行的是 npm 全局安装的 `debugtui.exe`，版本 0.8.4，SHA256 为 `D879688D0112AA554E765A6C987903618F3DA0410E2AE47BD753F3380C47FBBD`。完整矩阵、产物哈希和证据索引：[`artifacts/regression-0.8.4-20260922/report.json`](artifacts/regression-0.8.4-20260922/report.json)。测试期间工作区其他未构建改动不属于此固定 EXE 的回归范围。

实际工程目录为 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440`；GHS 2023.1.4、GDB 16.3、工程定制 OpenOCD、CMSIS-DAP SWD 5 MHz、THA6206 双 Cortex-R52。使用三个工程配置的隔离副本，用户 TOML 配置哈希保持不变。

| 范围 | 实板结果与证据 |
| --- | --- |
| Build / Download / 符号 | 实际 GHS Build、厂商 HEX 烧录及系统复位通过；DebugTUI 下载后自动重连两核，Run 命中 `_main`。`ROM.INT_VECTOR_TABLE` 和 `EX_CODE` 的 GDB `compare-sections` 均 matched。连接状态下 Build 也恢复双核符号、Watch 和禁用断点。 |
| Core0 / Core1 单独调试 | Core0 启动、Step/Next/Finish/StepI、断点及变量读取通过。Core1 在 Core0 初始化并运行后，重复命中 `DemoApp_MainCore1`，完成各类单步；Core1 暂停期间 Core0 的 AHB 计数器继续递增。未 examine 的 Core0 状态不能直接当作 STOPPED，验证使用计数器实际进展。 |
| 多核 | 27 项组控制核对通过：All/Core 的 Continue/Pause、断点联停、命中核自动切换、单步保持另一核 PC、重复 Run、共享复位和重连。源码断点默认当前核；升级/降级多核、条件/忽略次数同步、删除隔离和重连恢复通过。 |
| 断点 / 数据观察点 | 条件、忽略次数、批量启停、临时和硬件断点通过。Write、Read、Read-write 三类硬件观察点分别核对 GDB 类型和实际停止原因，不把三次 Write 当作三类覆盖。 |
| 数据和视图 | Watch 表达式、结构体、指针、32→64 项数组分页、错误行和移除通过；源码列表、栈帧、Locals、17 个系统寄存器、反汇编、Memory、GDB/AHB 读取及运行状态门控通过。SVD 的 BASETIMER0 展开显示实际 LOAD/VALUE/CTRL 等值。 |
| 运行中 Watch | 旧 `live_watch` 的五个双核采样窗口通过；切核、删除再添加、停止刷新和复位后采样正常。已安装 EXE 的可见 Watch 数值连续变化；界面设置 AHB 100 ms 后，十六进制 LIVE 样本为 52079、52347、52618、52953、53212（此处转为十进制列示），采样时两核均保持运行。 |
| 新增 Source 文本操作 | Windows ConPTY 驱动已安装 EXE，验证双击/拖选、键盘整行复制、右键 Copy、实际 Windows Unicode 剪贴板读回、Add to Watch、拒绝跨行表达式、F9 当前核断点命中。Core0/Core1 分别添加 `uart_cnt`，退出保存后各核恰有一项。原剪贴板已恢复。 |
| 配置和终端交互 | Setup 修改 timeout 为 9000、Ctrl+S 保存、F5 启动并命中 `_main`、F2/Esc 保持会话；Files/Asm/Memory 实际内容、Help、Appearance 保存、文件列表、80×24 / 45×12 / 180×50 布局及终端 F5/F6 通过。属于伪终端自动化，不是人工 VS Code 鼠标验收。 |
| 源码映射 | ELF 中的绝对编译目录映射到另一个含空格的本地目录，`main.c:105` 实际断点命中；ELF、profile、source_root 和映射目标的相对配置解析通过。连接时拒绝直接换 ELF，断开后重载通过。 |
| 负例和清理 | Build 返回 17、Download 返回 23 时不自动重连，可手动恢复；构建中 Quit 取消并正常退出。测试结束无 DebugTUI/GDB/OpenOCD/jtag 残留进程，3333/3334/6666 无监听。 |
| **未通过：退出后运行** | **`session.on_exit="resume"` 两次复现错误。** 当前实现先 `-exec-continue`，随后 `-target-detach` 被 GDB 拒绝：`Cannot execute this command while the target is running`；断开响应失败、状态为 FAULT。默认 `detach` 路径通过。证据：`exit-resume-repro/result.json` 和 `core1-prepare-core0/requests.json`；本次没有修改该实现。 |

初始实板存在 Flash 读取/启动失败，通过本次实际 Build、厂商 Download 和系统复位恢复。下载前 Flash 不可读，不能据此断言旧固件不匹配；下载后代码段校验通过。17:21 左右的 IDR/APB-AP 故障，用户确认同期有拔插、断电或复位操作；通过 Reconnect 恢复后继续测试通过，将其记录为中断恢复场景。

本轮使用 GHS MCAL 固件，未覆盖 Bao 固件、其他平台实板、冷上电时序、长时压力、CTI 硬件同时暂停和断点资源耗尽。Source 内编辑及外部编辑器自动构建下载链尚非本阶段实现内容。旧测试驱动的参数、坐标/时序和返回结构断言错误已纠正；原失败尝试及后续证据保留在回归目录，最终取值规则写入 `report.json`。

## 0.8.4：Source 选择、复制和加入 Watch（2026-09-22）

- 147 项单元测试、1 项任务集成测试与严格 Clippy 检查通过。覆盖拖选、双击选词、跨行/反向选择、Tab/中文、键盘扩选、右键菜单、窄终端、横向滚动、快捷键冲突和核心切换清理选区；行号栏及 F9 保留断点行为。
- Windows 原生剪贴板测试通过，实际读回中文、Tab 和 CRLF；测试后恢复原剪贴板。普通 UI 测试使用内存剪贴板，不修改用户剪贴板。
- 基于本机 `THA6XXX_MC_AS440` 工程的 `debug-core0.toml`、`debug-core1.toml` 和 `debug-multi.toml`，使用实际 Source/UI 事件处理与 Ratatui TestBackend，连接工程内的 GDB / OpenOCD 和 THA6206 实板：从实际 `Uart_Demo.c` 选择 `uart_cnt`，复制并加入当前核 Watch，通过值读取和每核列表隔离断言。测试禁用配置持久化，核对源码与 ELF 字节未改变，没有烧录固件。这是自动化 UI 到实板测试，不是终端鼠标手工验收。
- 额外 Reset/Run 验证未在 8 秒内命中 `_main`；OpenOCD 有 `DSCR.ERR`，GDB 校验 `EX_CODE` 时出现 target memory fault。完整固件启动流程未验证通过，不能将 Source 功能测试结果视为全部调试场景正常。证据与测试范围：`artifacts/source-selection-20260922/result.json`。

## 0.8.3：正式发布验证（2026-09-20）

- 正式版本重新执行 `cargo test --locked -- --test-threads=1`：137 个单元测试和 1 个 CLI 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 与 release 构建通过。记录：`artifacts/release-0.8.3/`。
- 使用正式 release EXE 重跑三个真实 GDB 的多核断点测试，99 个请求全部通过，涵盖核心关联、持久化、失败回滚和混合运行状态。记录：`artifacts/linked-breakpoints-1789906062625/verification.json`。
- 本版合入下面三个本地修复版本；THA6206 实板、实时 Watch 渲染和单核/多核回归的范围与证据见对应记录。

## 0.8.3-local.3：多核断点（2026-09-20，本地修复版）

- 137 个 Rust 单元测试、1 个 CLI 集成测试、严格 Clippy 检查通过。新增默认单核请求、非法核心/运行中拒绝、相同位置独立断点、不同 GDB 编号关联、回滚队列、鼠标选核/键盘应用、错误保留及 45×12 至 160×45 布局验证。
- `scripts/test-linked-breakpoints-gdb.cjs` 使用三个真实 GDB，99 个请求通过：部分核心关联、编辑/启停/批量操作、重连/进程重启、删除隔离、第二核缺失符号导致添加失败后的回滚、退回单核、混合运行状态及断点联停。证据：`artifacts/linked-breakpoints-1789900258546/verification.json`。
- 原多核 13 类场景与单核断点 60 请求回归通过：`artifacts/multicore-1789900841736/verification.json`、`artifacts/breakpoints-native-1789900841737/`。
- THA6206 Release 实板 61 请求通过：core0 / core1 默认独立新增、跨核关联、条件/忽略次数同步及重连；分别在 `Gpt_Notification_UartPrintf` 和 `DemoApp_MainCore1` 命中并核对两核实际 halted；取消关联、保留无关断点、删除后两核运行且 uart_cnt 增长。证据：`artifacts/multicore-breakpoints-20260920/hardware-release/result.json`。
- 实板 OpenOCD 分别报告 core.0 和 core.1 各 8 个硬件断点、8 个硬件观察点。此结果是每核资源报告，不是整片芯片共用配额，也不是对所有芯片的固定限制；Flash 普通源码断点由 GDB 自动使用硬件资源。

## 0.8.3-local.2：运行中 Watch 刷新（2026-09-20，本地修复版）

- 修复旧 `[live_watch]` 采样只写 Console、Watch 仍显示停止快照的问题。每次读取发送结构化 `live_watch` 事件；Watch 显示实时值和 `LIVE`，正常采样不再刷 Console。原始 8/16/32 位全局量保留 `<raw N-bit>` 标注，显式单项刷新策略优先。
- `cargo test --locked -- --test-threads=1`：132 个单元测试与 1 个 CLI 集成测试通过；Clippy `--all-targets -- -D warnings` 通过。新增真实 Watch 行渲染、进制转换、旧快照覆盖防护、过期/错核/已删除/停止后事件丢弃、失败恢复及显式刷新策略优先测试。
- THA6206 双核实板、GHS ELF、AHB_3、200 ms：core0 和 core1 的 `uart_cnt` 连续变化，与独立 AHB 读取区间一致；五个采样窗口均保持两个核运行，没有隐式 Pause/Continue。切核、状态查询、删除再添加、全核暂停及整片复位后恢复采样通过。记录：`artifacts/tha6206-live-watch-20260920/hardware-final/result.json`。
- 将上述实板 JSON 事件逐条交给实际 App 和 Ratatui Watch 渲染器，断言每个成功采样值及 `LIVE` 均出现在 Watch 数值行。回放测试通过，彩色缓冲保存在 `artifacts/tha6206-live-watch-20260920/ui-replay/`；这是实板事件的渲染回放，不是 VS Code 截屏。
- 停止后使用 GDB 快照。软件组暂停存在核间时间差，共享变量可能在先停核刷新后继续变化；实板校验在全核停止后显式 Refresh，再与 AHB 比较。AHB 读取仍受目标缓存一致性约束；本次验证对象是工程中该全局计数器，不代表任意结构体/表达式均可运行时求值。

## 0.8.3-local.1：THA6206 软件多核组控制（2026-09-20，本地修复版）

- `cargo test --locked -- --test-threads=1`：128 个单元测试与 1 个 CLI 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 通过。覆盖组范围/部分失败/断点焦点/非阻塞组等待/单次共享复位和全核刷新、UTF-8 服务日志分片、混合运行状态下的按钮可用性及窄窗口布局。
- `scripts/test-multicore-gdb.cjs target/release/debugtui.exe`：两个真实本机 GDB 的 13 类场景通过，包括组 Continue/Pause、断点联停与等待、单步隔离、重复 Run、范围切换、旧独立/单核行为及连接/服务失败清理。证据：`artifacts/multicore-1789892910291/verification.json`。
- THA6206 双 Cortex-R52、GHS 2023.1.4 ELF、工程 GDB 16.3 + 定制 OpenOCD、CMSIS-DAP SWD 5 MHz：最终 Release 122 请求、27 项核对通过。每核重复断点联停，单步不改变另一核 PC，运行中 AHB counter 递增，整片复位后两核 GDB PC 同为 0x08000000，独立/组模式切换及重连成功。证据：`artifacts/tha6206-multicore-fix-20260920/hardware-release-safe-regs/result.json` 与日志。
- 不承诺 CTI 硬件同时暂停；保留底层 GHS ABI 和部分复位寄存器访问诊断，详见该审计目录的 `REPORT.md`。未烧写或修改被测 ELF。

## 0.8.2：顶部启动按钮与返回配置（2026-09-20）

- 118 项 Rust 单元测试 + 1 项 shell 集成测试通过；Clippy `--all-targets --locked -- -D warnings` 无警告，Release 构建完成。新增覆盖顶部按钮在 45×12 / 80×24 / 120×36 下的命中区域、默认 Enter 启动、正在编辑字段的提交、无效配置不启动、Workspace / Esc 返回及草稿保留。
- Ratatui 单元格预览位于 `artifacts/ui-launch-0.8.2/`，检查 setup / setup-narrow / setup-compact / workspace。启动按钮固定在表单上方，主界面 Project 栏提供独立的 ← Setup。
- 安装后的 0.8.2 在真实 Windows 终端连接 STM32F429，使用 tools 中的 GDB + OpenOCD。Enter 从默认启动按钮连接；鼠标点击 ← Setup 后，TUI / GDB / OpenOCD PID 保持不变。将 timeout 改为 9000 后通过 Workspace 返回，再打开配置，草稿仍在且磁盘仍为 8000。
- 在字段编辑状态输入 9500 并按 Ctrl+R，无需先按 Enter：配置成功保存，旧 GDB / OpenOCD 清理后重新连接；TUI PID 始终为 9264。GDB PID 为 37128 → 26436，OpenOCD 为 18340 → 30860。重新连接后 `p xTickCount` 返回 82729393，Watch 及两个禁用的代码/读数据断点记录成功恢复。
- 再次通过 F2 打开配置、鼠标点击顶部 Start 完成第二次重启，GDB / OpenOCD PID 分别为 35376 / 36192；随后 F2 / Esc 返回工作区正常。Ctrl+Q 退出码 0，三份 session 日志均包含连接、`monitor resume` 和 GDB 退出，结束后无调试进程及 3333/6666 监听。证据：`artifacts/terminal-launch-0.8.2/verification.json`、该目录的进程快照和 `logs/`。测试使用单独配置文件，没有改动用户工程配置或下载固件。
- 本地 npm 全局安装验证 `debugtui --version` 为 0.8.2；安装后 EXE 与 release SHA256 均为 `1E29FF6A70732403E2AF7D51F94B00A401D6DA69E096B2D32238B4F08A47691E`，EXE 大小 3,437,056 字节。

## 0.8.1：断点开关、数据断点与保存恢复（2026-09-19）

- 114 项 Rust 单元测试 + 1 项 shell 集成测试通过；Clippy `--all-targets -- -D warnings` 无警告，Release 构建完成。新测试覆盖勾选禁用/启用、重复操作抑制、Delete 与输入隔离、数据读写模式、失败保留编辑器、核心切换和列表选择、45×12 / 80×24 / 140×40 布局、旧配置与详细断点的往返保存。
- `scripts/test-breakpoints-gdb.cjs` 使用真实本机 GDB：条件/忽略次数/实际命中计数、临时断点、写数据断点、强转地址表达式、无效条件回滚、部分创建失败清理、批量启停、立即保存、重连与进程重启恢复、无法恢复的记录保留及明确删除。`--multi` 使用两套真实 GDB，验证启停只影响当前核心，两个核心分别保存和恢复。
- STM32F429 + J-Link V8 / tools OpenOCD：同一脚本 `--hardware` 先执行 `compare-sections .text` 确认固件匹配；不重新下载固件。代码断点、条件/忽略次数、临时/硬件代码断点，以及 `*(unsigned int *)&xTickCount` 的 **Write / Read / Read-write** 三种数据断点均实际触发暂停；禁用与重新启用保持编号，三类禁用数据记录与条件代码断点在重连和重启后完整恢复。首次通过记录：`artifacts/breakpoints-stm32-1789827910744/verification.json`（75 个请求）。
- 实测发现远程 GDB 在自动跨过被忽略的断点时偶尔拒绝 `-thread-info`；已修复为在原超时内继续等待，未伪造 STOPPED。`scripts/test-pause.ps1` 新增 query-rejected 场景，六种暂停竞态/超时测试通过；真实运行目标仍按期报超时。
- 原 Watch 树/补全/强转表达式和 11 组多核回归通过：`artifacts/watch-tree-1789828265871/`、`artifacts/multicore-1789828276937/verification.json`。多核硬件仍未做实板验收，当前硬件是单核 STM32F429。
- 实际 Ratatui Buffer 渲染预览：`artifacts/ui-breakpoints/{breakpoints,breakpoints-narrow,breakpoint-editor}.png`；逐项检查标签、开关、对话框及窄终端命中区域。
- 安装后的 0.8.1 再次通过本机断点 60 请求、双 GDB 23 请求和 STM32 75 请求：`artifacts/breakpoints-native-1789828383385/`、`artifacts/breakpoints-multi-1789828387086/`、`artifacts/breakpoints-stm32-1789828381945/`。实际终端用 Space 启用/禁用同一代码断点，通过编辑器选择 Read 并创建 xTickCount 数据断点，再用鼠标点击勾选框禁用；两个禁用记录均立即写回测试工程。日志：`artifacts/terminal-breakpoints-0.8.1/logs/session-20260919-223353-374.log`。Ctrl+Q 返回 0，测试结束后 debugtui / GDB / OpenOCD 进程及 3333/6666 监听端口均为 0。
- 本地全局 npm 安装完成：`debugtui --version` 为 0.8.1；release 与安装后 EXE SHA256 一致：`3D621BC656EC56DC73DB4CEE8343DD2EF045EE5B88F6C53C854475ACC842D84F`，文件 3,432,960 字节。无需新增运行时依赖。

## 0.8.0：核心按钮、内存通道和可见项实时刷新（2026-09-19）

- 107 项 Rust 单元测试 + 1 项 shell 集成测试通过；`cargo clippy --all-targets -- -D warnings`、`cargo build --release --locked` 通过。测试涵盖核按钮命中/窄窗口、核心底色及 Source/Asm 内容切换、刷新策略持久化/分核隔离、延迟响应丢弃、轮询间隔、隐藏/删除项、运行状态访问门控、SVD 副作用寄存器禁用轮询、手动位域刷新父寄存器，以及内部 TCL 回显识别。
- 双真实本机 GDB + 模拟 TCL：`artifacts/multicore-1789825041872/verification.json`，11 组回归通过，包括两核独立状态、失败回滚、单核兼容、多个 CTI 初始化顺序、借核通道的核心限制和共享 AHB 在某一核运行时读取。**没有多核实板，CTI 触发/多 AP 硬件路由尚未做实板验收。**
- Native GDB 内存通道测试：`scripts/test-memory-access-gdb.cjs`，验证有符号/浮点/64 位类型、嵌套成员和地址强转、无地址表达式拒绝、TCL 超时/断线/错误恢复、64 位端序、禁止隐式停核、读完仍可 Pause/Step/Asm。release 记录：`artifacts/memory-native-1789825035841/verification.json`，已安装 exe 记录：`artifacts/memory-native-1789825387920/verification.json`。
- STM32F429 实板：J-Link V8 探针 + tools 内 xPack OpenOCD，SWD 1000 kHz；建立 `stm32f4x.cpu` 和 AP0 `stm32f4x.ahb`。先发现旧 `Debug/FreeRTOS_Project.elf` 与板上固件不符；改用 `build/FreeRTOS_STM32F429.elf`，GDB `compare-sections .text` 报告 **matched**。未下载或改写固件。
- 实板暂停时，GDB、CPU target 和 AHB 对 `xTickCount` 的值一致；AHB/GDB 的 RCC.CFGR、GPIOB.MODER、DBGMCU.IDCODE 一致。运行时按 100 ms 请求间隔采样 20 次并读 GPIOB，tick 持续增长、状态保持 RUNNING、generation 不变、没有新增 GDB 命令；默认 GDB 和 stopped-only Core 通道被拒绝。之后 Pause、单指令步进、反汇编、系统寄存器和 Watch 正常。自动启动和退出自有 OpenOCD 已验证，退出后 3333/6666 端口释放。
- 已安装 TUI 的真实终端测试：`artifacts/terminal-0.8.0/`。从 Watch 的 `f → r` 菜单选择 AHB，启用 100 ms 刷新并保存；确认 `[ui.refresh."single|watch:xTickCount"]` 写回，运行后 LIVE 数值持续变化，Ctrl+Q 清理退出。测试发现并修复 OpenOCD 将 TCL 返回值同时回显到 GDB target 和自有 Server stderr 导致 Console 刷屏：内部返回加唯一标记，统一归入 diagnostic，原始目标输出仍保留。
- 原 Watch 树/自动补全/日志轮转、ARM/RISC-V/x64 模拟 MI、五种 Pause 竞争场景均回归通过。release 记录：`artifacts/watch-tree-1789825034262/`、`completion-native-1789825040444/`、`logs-native-1789825043464/`、`environment test 工程 20260919-213719/`、`pause-20260919-213726/`。
- UI 预览从 Ratatui 实际缓冲导出并检查：`artifacts/preview-0.8.0/core0.png`、`core1.png`、`refresh-settings.png`；不是设计稿。npm 包不包含 OpenOCD/GDB/Node/Python，tools 独立校验打包通过。
- 旧 J-Link Server 实板尝试未通过：当前 USB 设备为 WinUSB/libwdi (`oem8.inf`)，SEGGER V7.94e 在 TUI 内及独立 CLI、SWD 4000/1000 kHz 下均报告 `Could not connect to J-Link`。未更改系统 USB 驱动；本次实板通过项均使用 OpenOCD，不能宣称 J-Link Server 实板回归成功。记录：`artifacts/svd-hardware-1789825411709/`。
- 本地 FreeRTOS 工程 `debug.toml` 已将 ELF 改为匹配的 build 文件、Tools profile 改为 `debug-env-openocd.toml`，其余工程设置保留；原配置备份为 `artifacts/debug-before-live-0.8.0.toml`。Git 提交和发布不在本次操作范围。
- 最终安装版本 `debugtui 0.8.0`；release 和 npm 全局安装 exe 的 SHA-256 均为 `7D2B0539E6FF5CCFC8C6B0B650F56951BCE31B3355BB485318E5C018185E20C5`。最终已安装 exe 实板 70 个请求全部通过：`artifacts/memory-stm32-1789825648786/verification.json`。
- 最终终端复测：`artifacts/terminal-0.8.0/logs/session-20260919-214757-727.log`，恢复已保存的 Watch 100 ms 策略，RUNNING 下 LIVE tick 持续增长；隐藏 Watch 后不继续读取它，在 Peripherals 展开 ADC1、为 CR1 选择 AHB 和 500 ms 策略，显示 `0x0 hex LIVE`。100 次 AHB 采样时 Console 内部回显为 0，寄存器读和 Watch 读均不写硬件；退出码 0。当前采样时 TUI 工作集约 12.22 MiB / private 5.59 MiB（含加载 F429 SVD，不含 GDB/OpenOCD，非性能保证）。

## 0.6.6：Watch 面板直接增删

- Watch 底部变量输入框增加 **+ Add** 按钮，复用自动补全及 Enter 提交流程；空输入点击聚焦，重复提交不重复排队，失败保留输入，旧响应不清除新草稿。
- 每个变量名称行右侧增加 **×**，点击直接删除对应表达式，无需预先选中；滚动至只有数值行可见时仍保留该项删除入口。点击目标按表达式识别，避免异步列表刷新后误删相邻项。原有 Delete / Del Remove 保留。
- 78 项单元测试 + 1 项实际 shell 集成测试通过，Clippy 零警告，Release 构建通过。新增测试覆盖鼠标增删、运行状态、错误重试、输入隔离、Help / Locals 焦点、45×12 至 160×42 布局、滚动和刷新后的点击目标。
- 原生终端 + 真实本机 GDB 实测：断点停在 main，点击 + Add 聚焦、输入 `coun` 并 Tab 补全为 `counter`，点击添加显示 17；再点击添加 `counter_total` 显示 23；直接点第二项的 × 仅移除第二项，再点第一项的 × 清空。整个 Watch 增删过程未输入调试命令。正常退出码 0，保存配置 `watch = []`。
- 交互记录：`artifacts/ui-0.6.6/terminal/interactions.json`。使用独立本机测试程序，未连接物理 MCU。

## 0.6.5：Watch 删除与焦点修复

- 修复前用三个回归用例复现：删除末项后选中行越界，无法继续删除；删除前面的项目后选择偏移到其他表达式；Console 历史焦点仍触发 Watch 的 Delete。修复后均通过。
- Watch 名称行和数值行对应同一个选择；列表变化后按表达式保持选择，被删除时移动至相邻项并修正滚动位置。删除请求完成前抑制重复发送，失败后可重试，按住 Delete 的重复事件不会连续清空列表。
- Watch 标题增加 Del Remove 鼠标入口；输入框、Console、Help 焦点不误删 Watch，输入草稿保留。测试覆盖滚动后的名称/数值行、鼠标按钮、连续删空、错误响应和运行中移除。
- 73 项单元测试 + 1 项实际 shell 集成测试通过，Clippy 零警告。真实本机 GDB 回归额外覆盖停止时删除两项、运行中删除一项：删除没有额外 MI 请求，没有暂停目标或改变变量；重连并再次停止后已删除项未恢复，保存的项目 Watch 列表为空。
- 原生 GDB 验证记录：`artifacts/completion-native-1789653131310/verification.json`。测试仅操作独立本机测试程序，不连接物理 MCU。
- 本地安装版真实终端验证：断点暂停后选中最后一项，连续两次 Delete 分别移除 `counter_pair.value`、`counter_total`，再鼠标点击 Del Remove 移除 `counter`；空列表再次 Delete 正常提示，退出码 0，保存的配置为 `watch = []`。交互输出：`artifacts/ui-0.6.5/terminal/interactions.json`。

## 0.4.2：Source root 命令与独立 Project 操作区（2026-09-17）

- 启动页在 Source root 后增加 Build command / Download command，保留引号并保存至项目 `[tasks]`。配置解析、相对路径、tools 下载动作回退和未生成 ELF 时进入工作台均有回归覆盖。
- 主界面顶部独立 Project 栏显示 Build / Download，与 Run / 单步工具栏分开；45×12 至 160×42 均可见。未配置时禁用；执行中抑制重复任务；下载确认显示具体命令及工作目录。
- 40 项单元测试 + 1 项 Windows 实际子进程集成测试通过，Clippy 零警告，Release 构建通过。子进程测试覆盖中文/空格工作目录、带引号脚本路径和参数、stdout/stderr、非零退出码、超时及 Ctrl+Q 取消。
- 本机 GDB 完整测试：先从 Source root 实际 GCC 编译，再连接、断点运行；已连接时重新 Build，释放 GDB 后成功替换 EXE 并重连；配置的 Download 测试脚本执行成功后再次重连。两次重连后的运行都命中恢复的 main 断点。记录：`artifacts/ui-0.4.2/project-tasks/events.jsonl`。
- 实际 Windows 伪终端验证 F2 配置展示、点击 Build 编译与重连、点击 Download 确认并运行脚本、正常 Exit。记录：`artifacts/ui-0.4.2/terminal-interactions.json` 与 `interactive-logs/`。
- 原有真实 GDB 调试回归（断点、变量、单步、寄存器、反汇编、清理）通过：`artifacts/native-gdb-20260917-010958/`。彩色布局预览：`artifacts/ui-0.4.2/`。

Download 使用本地验证脚本检查执行目录、参数与会话恢复，没有烧写 STM32，也未重测探针长会话问题。

## 0.4.1：单步命名、Exit 与 Help（2026-09-17）

- 工具栏改为 Step In / Step Over / Step Out，继续分发 `step` / `next` / `finish`；F11 / F10 / Shift+F11 保持原有行为。
- 新增 Exit，复用 Ctrl+Q 的会话清理与退出流程。45×12 极矮窗口也保留 Exit / Help；退出等待期间禁用重复点击和快捷键请求，demo 的 Exit 同样结束程序。
- CommandList 改为 Help，包含 Commands / Shortcuts 两页，支持点击页签和 Tab 切换；`?` / `:help` 直接打开快捷键页。命令列表执行、带参数命令填入 Console 的行为保持不变。
- 38 项 Rust 测试、Clippy 零警告和 Release 编译通过。新增测试覆盖不同状态的 Exit 分发、取消标志、重复退出抑制、demo 退出，以及两种窗口尺寸下 Help 页签、键盘导航和命令执行。
- 真实 Windows 伪终端 + 本机 GDB：断点停在 main.c:4；点击 Step In 进入 helper.c:2；点击 Step Out 返回 main.c:4；两次 Step Over 分别到 main.c:5、main.c:6，第二次越过函数调用。Help 的鼠标页签和 Tab 切换正常；点击 Exit 返回码 0，日志确认 `-target-detach` 和 `-gdb-exit`，测试进程退出。
- 完整终端交互与 MI 日志：`artifacts/ui-0.4.1/native-interactive/`；Ratatui 缓冲彩色预览：`artifacts/ui-0.4.1/`。

本轮验证使用独立的本机程序，没有重新验证 STM32/J-Link 长会话问题。

## 0.4.0：终端视觉层改造（2026-09-17）

- 深色分层面板、真彩色主题、图标工具栏、状态徽标、选中/悬停反馈、圆角 Console 和弹窗；全行执行位置与变化值背景；启动配置页统一视觉样式。新增轻量 C 类语法着色，跨行注释状态只在当前文件载入时计算。
- 36 项 Rust 测试、Clippy 零警告与 Release 构建通过。保留原有点击、滚动、文件标签、Console 和数据加载回归；新增验证全行背景、弹窗不透出底层字符、悬停不发送调试命令、45×12 至 180×50 的控件边界与输入焦点，以及字符串/Unicode/跨行注释着色。
- 极矮终端保留 Continue / Pause / CommandList 三个主要按钮，为源码留出可见空间；其余命令继续由 CommandList 提供。80×24 及常规窗口显示完整工具栏。
- 真实本机 GDB 回归验证连接、断点、变量求值、Step、x86 寄存器、反汇编和退出清理。记录：`artifacts/native-gdb-20260917-002657/`。
- Windows 伪终端真实交互：Console 输入 `:break main`、`:run`，在 main.c:4 停止；发送 F11 后进入 helper.c:2，两个源码标签保留；Ctrl+Q 正常退出。完整 ANSI 输入/输出与 GDB 日志：`artifacts/ui-0.4.0/native-interactive/`。
- 真彩色模式独立验证：仅在测试子进程去除 `NO_COLOR` 并设置 `COLORTERM=truecolor`，启动实际 EXE，初始画面捕获 149 次 RGB 颜色控制指令、14 个不同前景/背景颜色序列；Ctrl+P 命令弹窗正常。程序继续遵循调用者的 `NO_COLOR`，不修改系统或用户终端设置。
- 最终 EXE 1,479,680 字节，较 0.3.4 的 1,462,784 字节增加 16,896 字节（16.5 KiB，约 1.16%），没有增加 Cargo 或运行时依赖。
- 本机单次 5 秒空闲采样：真彩色 demo 命令面板私有内存 1.26 MiB、工作集 6.42 MiB，CPU 时间增量 0 ms；真实 GDB 暂停会话 UI 私有内存 1.72 MiB、工作集 7.18 MiB，CPU 增量 0 ms。这是本机短采样，不代表所有工程或操作负载。
- 从真实 Ratatui 单元格缓冲导出的彩色预览：`artifacts/ui-0.4.0/`，包含 workspace / console / narrow / compact / commands / files / assembly / setup；PNG 用于检查视觉效果，不进入运行时安装包。

本轮修改视觉与本地输入反馈，没有修改 GDB 会话命令实现、tools 或用户固件，也没有重新验证或修复下述 STM32/J-Link 长会话问题。

## 0.3.4：Source 多文件标签与变量区分割线（2026-09-16）

- Watch / Locals 上方恢复贯穿右侧面板的横向分割线。
- Source 文件通过暂停/断点停止、栈帧切换、Files 和 `:open` 打开后保留为标签，可点击切换或 `×` 关闭，独立记住浏览位置。真实路径去重、同名文件路径后缀、中文/长文件名显示及关闭按钮命中均有覆盖。
- `[<]` / `[>]` 与标签栏滚轮支持浏览隐藏标签，`[Files N]` / Ctrl+O 打开可搜索文件列表，Ctrl+PgUp/PgDn 切换，Ctrl+W 关闭。标签仅缓存元数据，当前源码文本仍限制为单文件 2 MiB / 30000 行。
- 33 项 Rust 测试全部通过；Clippy `--all-targets --locked -- -D warnings` 通过；Release 构建通过。新增测试覆盖 120 个标签、45×12 至 160×45 的缩放与活动标签可见性、筛选隐藏文件、关闭活动/最后一个标签、关闭后普通刷新不重开、新停止重新打开、源码路径别名的断点定位，以及无源码停止时保留标签但不冒充旧执行位置。
- 真实本机 GDB + Windows 伪终端验证：在 main.c 设置断点运行，Step 进入 helper.c 后显示两个标签；点击 main.c 只浏览源码，GDB 仍停在 helper.c；点击 helper.c 的 `×` 后，下一次 Step 停止重新打开该文件；打开 `[Files 2]`，输入 `main` 筛选并切换成功。正常 Ctrl+Q 退出。
- GDB 记录：`artifacts/ui-0.3.4/native-tabs/logs/session-1789572980354.log`。第 55、71 行为两次 Step；停止位置分别为 helper.c:2 和 helper.c:3。浏览/关闭标签没有发送 `-stack-select-frame` 或 `-break-delete`。终端输入与 ANSI 输出保存在 `artifacts/ui-0.3.4/native-tabs/terminal-interactions.json`。
- 布局预览：`artifacts/ui-0.3.4/source-tabs.txt`、`source-tabs-narrow.txt`、`open-files.txt`。
- Release EXE SHA256：`06E7F0F483E36C0AF8B8FC5351E98058D80739081D69DB21571B58034BFE4B2F`。

本轮为源码浏览交互改动，真实调试验证使用独立的本机测试程序；未重新验证下述 STM32/J-Link 长会话问题。

## 0.3.3：版本标题、统一滚动与 Console 输入（2026-09-16）

- 工作台及启动配置页显示 `DebugTUI v0.3.3`，版本来自 Cargo 元数据。
- 左侧 Source / Asm / Files / Log 均有滚动轨道，内容超过一页时显示可拖动滑块。右侧为 Regs / Stack / Memory / Breaks，下方为 Watch / Locals，标签独立切换。右侧列表也复用滚动逻辑。
- Console 内固定显示 `gdb>` 输入框，鼠标点击或 `/` 聚焦。支持 GDB 原生命令、`:命令`、连续提交、↑ / ↓ 历史、Esc 退出输入；输入期间保留功能键调试操作。
- `cargo test --locked`：28 项全部通过；`cargo clippy --all-targets --locked -- -D warnings`：通过；Release 构建通过。新增覆盖标签位置及窄窗口可达性、Asm/Files/Log 首尾滚动及标签位置保留、滚动后的文件点击定位、Log 旧记录浏览和恢复跟随、Console 请求分发及连续输入/历史/长 Unicode 命令。
- 布局预览：`artifacts/ui-0.3.3/`。覆盖 45×12、80×24、120×36、180×50 渲染；实际 Windows 伪终端验证 Console 鼠标聚焦与提交。
- 已安装版本连接真实本机 x86 GDB，在独立 sample.c 测试程序执行 `:break main`、`:run`、`p/x counter`、`next`，再用 ↑ 历史重发 `p/x counter`：结果依次为 `0x1`、`0x2`。Asm 自动加载实际反汇编，鼠标拖动至末尾可浏览后续指令；Files 和 Regs 切换正常。日志：`artifacts/ui-0.3.3/native-console/session-1789571838149.log`，第 54/70/86/102/104 行分别记录求值、Next、再次求值、反汇编及文件列表请求。Ctrl+Q 正常退出并关闭自有 GDB。
- `scripts/test-native-gdb.ps1`：真实本机 GDB 的启动、断点、变量求值、Step、动态寄存器、反汇编和清理全部通过；记录位于 `artifacts/native-gdb-20260916-231700/`。
- `scripts/test-npm.ps1`：隔离安装、升级、CMD/PowerShell 入口、配置保留、卸载及重装全部通过。已通过 npm 全局安装更新本机入口，`debugtui --version` 返回 `debugtui 0.3.3`。
- 安装 EXE 与 Release EXE 的 SHA256 一致：`E0573E7407C802BBB02733D2C8F8009EAC11E3F90C91E46B3A9C8CB633B77B93`。

本轮验证的是界面交互和真实本机 GDB 命令链路，没有重新测试或修复下述 STM32/J-Link 长会话访问异常，没有更换调试环境工具或修改用户固件。

## 追加隔离实验：Next / Step / Stepi / Monitor（2026-09-16）

**新结论：在本套环境中，不发送任何单步命令，停在断点后保持连接 60 秒，也能复现底层读取失效。不能继续将根因概括为 F11 或源码单步算法。根因部件仍未确定，尚未修复。**

### 条件与方法

- 直接运行 tools 中的 ARM GDB/MI 与 J-Link GDB Server 7.94e，绕过 DebugTUI；同一 V8 探针、SWD 4000 kHz、STM32F429IG、同一 ELF。
- 每组独立启动 Server/GDB，复位到相同程序状态，检查 Flash 的 6 个只读段与 ELF 匹配，然后停在 `user_task.c:40`（`0x080007e2`）。不下载固件。
- 每轮先 Continue 到该断点，再只执行该组的一种命令；核对 PC、SP、xPSR，不能只以 `^done` / `^running` 判断成功。
- 预期：Next 到 `0x080007e6` / `user_task.c:44`；Step 到 `0x080005fc` / `bsp_led.c:32`；Stepi、`monitor step` 到 `0x080005f8` / `Led_Task` 入口。Monitor 每轮直接读取 `monitor regs`，并刷新 GDB 寄存器缓存后交叉读取。
- 日志逐行记录距启动的毫秒时间。发生错误先保留日志并尝试读取寄存器、CPUID、DHCSR、CFSR、HFSR，不在组内复位/重连恢复后继续计数。

### ① 四种操作独立对照

短会话：每种方式 **3 个会话 × 100 次全部通过**，合计 1200 次；每会话约 18–23 秒。初步 5 次校验不计入这些数字。

进一步将每种方式的单会话上限提高至 500 次：

| 分组 | 异常前完整通过轮数 | 首个异常时间（距 Server 启动） | 实际失败阶段及证据 |
|---|---:|---:|---|
| Next | 241 | 52.522 s | 第 242 轮 Continue 回断点时异常，Server 的 xPSR 等寄存器读数失真，随后等待停止超时 |
| Step | 249 | 53.325 s | 第 250 轮 Continue 时 Server 报 `No more breakpoint resources left`、断点插入失败；GDB 返回 `Command aborted` |
| Stepi | 268 | 52.575 s | 第 269 轮 Continue 时同样发生断点插入失败和 `Command aborted` |
| Monitor Step | 310 | 52.893 s | 第 311 轮 Continue 后 SP/LR/PC/xPSR 均为 `0xab78`；直接 `monitor regs` 返回全零，系统寄存器读取 Failed |

**注意：本次四个长会话的失败阶段都是重新 Continue 定位断点，并非可以据表格认定四种单步命令本身都失败。** 不同操作次数却在接近的时间失效，因此追加静置对照。

Step/Stepi 的完整协议还显示：同一 token 先返回 `^running`，随后返回 `^error,msg="Command aborted."`。首次测试脚本只收到前者时报告停止等待超时，原始日志保留了后者；脚本现已识别这种后续错误。不能把这两次解释成 CPU 正常运行而单纯丢失停止通知。

### ② 绕过源码单步与静置对照

| 测试 | 结果 |
|---|---|
| `monitor step` + `monitor regs` | 短会话 300 次通过；长会话前 310 次通过，失效后直接 monitor 读取也不正常 |
| 停在同一断点，不发送任何命令 30 秒 | 读取仍正常，后续 10 次 monitor 单步全部通过 |
| 同样保持 60 秒，第 1 次 | 尚未执行单步，`monitor regs` 已返回全零；刷新缓存后 GDB 的 PC/SP/LR/xPSR 均为 `0x5428` |
| 同样保持 60 秒，第 2 次 | 尚未执行单步再次失败；GDB 的 PC/SP/LR/xPSR 均为 `0x9768` |

60 秒失败现场直接读取 `0xE000ED00`（CPUID）、`0xE000EDF0`（DHCSR）、`0xE000ED28`（CFSR）、`0xE000ED2C`（HFSR）均返回 `Failed`。这是 monitor 命令的输出内容；即使 MI 外层是 `^done`，也不能算硬件读取成功。

这将问题范围收敛为 **连接/暂停持续一段时间后，Server/探针/目标侧的访问状态失效**，而不是 F10/F11 专属操作。52–53 秒为本次四个连续操作会话的观测值，不等于已经证明存在固定超时设置。仍需区分 J-Link Server/DLL、探针固件/硬件、目标复位/调试状态及连接条件；未验证的软件或硬件原因不得标为已确认。也不能把错误读出的 PC 当作真实程序执行地址。

### 复现与证据

脚本仅用于开发测试，不进入运行时安装包：

```powershell
node scripts/test-step-isolation.cjs <ELF> 100 3
node scripts/test-step-isolation.cjs <ELF> 500 1
node scripts/test-step-isolation.cjs <ELF> 10 1 monitor 30000
node scripts/test-step-isolation.cjs <ELF> 10 1 monitor 60000
```

每个会话保存 `trace.log`、`timeline.json`、`result.json`，总目录有 `results.json`。

- 1200 次短会话：`artifacts/step-isolation-2026-09-16T13-50-39-156Z/`
- 长会话四组：`artifacts/step-isolation-2026-09-16T13-54-58-531Z/`
- 首次静置 60 秒：`artifacts/step-isolation-2026-09-16T13-59-37-549Z/`
- 静置 30 秒对照：`artifacts/step-isolation-2026-09-16T14-01-10-862Z/`
- 重复静置 60 秒：`artifacts/step-isolation-2026-09-16T14-01-45-671Z/`

ELF SHA256：`076B3D1D9CC4DC04BBBBB6269E816673BA78CBF9730E54BF8B9A4718D842FCDD`。本轮未修改 TUI 执行逻辑、工具默认配置或用户工程配置。

## 0.3.2：断点后 Step / Next 异常的实板复现

日期：2026-09-16。结论：**底层异常已经复现，尚未修复；0.3.2 修复的是异常位置显示，不能作为底层稳定性修复。** 未更换工具二进制、未修改默认 SWD 配置、未重新烧写固件。

用户日志：`G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/debug-logs/session-1789564435915.log`。

- 断点 `user_task.c:40` 正常命中 9 次后，日志第 1573 行发出 `105-exec-continue`。随后 J-Link 报告断点移除/设置失败，GDB 提示程序不可写，PC 变为 `0x00006978`、函数 `??`，多个寄存器返回相同异常数值。
- 第 1926–1934 行的三次 `-exec-step` 均返回 `Cannot find bounds of current function`。这是异常 PC 之后的结果；本份日志没有 `-exec-next`，不能声称记录到了 Next 的按键操作。
- 测试前 `compare-sections -r` 的 6 个只读段全部匹配板上 Flash。正常时 Step 进入 `Led_Task` 的 `bsp_led.c:32`，Finish / Next 返回 `user_task.c:44`；当前 ELF 未启用 `VTASKDELAYUNTIL`，实际延时代码在 44 行而非 42 行。

| 对照 | 实际结果 |
|---|---|
| 已安装 0.3.1、默认 4000 kHz | 连续命中断点 40 次通过；第 14 轮 Step/Finish/Next 序列中 Next 后 PC 变为 `0x00007efc`，PC/SP/LR/xPSR 返回相同异常数值 |
| 临时覆盖为 1000 kHz | 第 64 次继续到断点时 PC 变为 `0x00006920`；降频没有解决 |
| 不启动 DebugTUI，直接 GDB CLI + J-Link | 24 轮完整序列通过，第 25 轮 Step 后 PC 变为 `0x0000a490`、无调用帧；复现不依赖 TUI/MI 调度 |
| 直接 GDB，关闭 Flash 断点并使用 hbreak | 26 轮完成后再次出现异常寄存器读数并持续等待；终止本次测试的 GDB，Server 随连接关闭退出；该设置也未解决 |
| 失败后刷新 GDB 寄存器缓存 | 异常读数仍存在；读取 `0xE000ED28` 故障状态寄存器失败 |
| 本地安装 0.3.2 短程回归 | 10 次继续到断点 + 10 轮 Step/Finish/Continue/Next/Next/Continue 通过，检查了函数、源码行、xPSR 和异步停止通知；没有使用 wait_stopped 掩盖通知问题 |
| 通用回归 | 24 项 Rust 测试、Clippy 零警告、Release 构建、5 个 Pause 异常分支通过；安装 EXE 与构建 EXE 的 SHA256 相同 |

上述结果把问题范围缩小到 GDB 以下的调试链路及目标状态，尚不能区分 J-Link Server/DLL、探针固件/硬件、SWD/USB 连线或目标侧问题。后续需用另一探针/线缆或另一套已验证驱动做单变量对照，不能把重连恢复、短程测试通过当作长期故障已修复。

0.3.2 的界面修复：没有源码位置的停止事件会清除上一次源码视图，并显示实际 PC、函数名与检查提示；顶部位置取自 GDB 当前帧，手动浏览文件不会改写停止位置；Step/Next 错误附带执行地址和函数。未自动改为 stepi、复位或重连。通用 TUI 没有加入 STM32/J-Link 特例。

复测脚本：`node scripts/test-step-hardware.cjs <ELF> [EXE] [轮数=40] [SWD_KHZ]`。该开发测试针对当前 STM32 FreeRTOS 固件、创建隔离配置，不改用户工程配置；生成命令结果、停止位置及完整 MI/Server 日志，不进入运行时安装包。

证据目录：

- 4000 kHz 复现：`artifacts/step-hardware-2026-09-16T13-22-58-427Z/`
- 1000 kHz 复现：`artifacts/step-hardware-2026-09-16T13-25-11-193Z/`
- 直接 GDB 与硬件断点对照：`artifacts/step-direct/`（含 GDB 命令文件、GDB 输出及 Server 日志）
- 安装版短程：`artifacts/step-hardware-2026-09-16T13-31-06-255Z/`
- Pause 回归：`artifacts/pause-20260916-213107/`

对照命令依据：[SEGGER 的 GDB Server monitor 命令](https://kb.segger.com/J-Link_GDB_Server)、[GDB Step / Next / Stepi 语义](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Continuing-and-Stepping.html)。

## 0.3.1：Pause 超时后的状态核对

日期：2026-09-16。已本地安装 0.3.1，安装 EXE 的 SHA256 与 Release 构建相同。

用户报告设置源码断点、继续运行后 Pause 等待停止超时。旧版实板连续 50 轮未重现原始间歇故障；在“interrupt 返回确认、GDB 线程已停止，但缺失停止通知”的受控模拟中，旧版稳定出现同样的等待超时，退出清理也重复等待。不能据此断言原始故障一定由 J-Link 或 GDB 丢通知引起。

修复为：保留异步停止通知处理，同时每 250 ms 使用标准 `-thread-info` 核对线程状态；只有线程确认为 stopped 才同步并刷新上下文。若仍运行，最多补发一次 interrupt；无法暂停时仍有界报错，不伪造 STOPPED，不隐式复位或重连。退出清理使用同一逻辑。日志增加 MI 异步记录，便于继续追踪间歇故障。协议依据：[GDB 的异步执行与线程状态查询](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Asynchronous-and-non_002dstop-modes.html)。

| 验证 | 结果 |
|---|---|
| 受控旧版复现 | 缺失停止通知时复现 `Timed out waiting for target to stop` |
| 5 个异常分支 | 缺失通知、目标已停止、通知延迟、首次 interrupt 未生效均可在原连接恢复并继续单步；真正持续运行时仍正确超时 |
| 安装版 STM32 循环 | 200 轮断点/next/continue/pause 全部通过；重连次数 0；最大 Pause 响应 141 ms |
| 实板完整回归 | 断点、观察点、内存、反汇编、帧切换、复位、重连、run、错误清理均通过，无重烧写 |
| 通用环境 | ARM/RISC-V/x64 模拟通过；真实本机 MinGW GDB 回归通过 |
| 编译检查 | 23 项单元测试通过，Clippy 全目标零警告，Release 构建通过 |

证据：旧版 `artifacts/pause-20260916-210354/`；安装版模拟 `artifacts/pause-20260916-211048/`；200 轮实板 `artifacts/pause-hardware-2026-09-16T13-10-40-349Z/`；完整实板 `artifacts/hardware-20260916-211207/`。

复现：`scripts/test-pause.ps1`；`node scripts/test-pause-hardware.cjs <ELF> [EXE] [循环次数]`。脚本仅用于开发测试，未加入运行时分发。发生现场问题时，可用 `debugtui --project . --log-dir ./debug-logs` 保存 MI 日志。

## 0.3.0：工作台布局与鼠标交互

日期：2026-09-16。本轮编译并安装到本机全局 npm 入口，未发布远端 Release。

| 验证 | 结果 |
|---|---|
| Rust / Clippy | 23 项单元测试通过；全目标 Clippy 零警告；Release 构建通过 |
| 布局 | 45×12、80×24、120×36、180×50 渲染通过；右侧五个标签切换保留源码，无底部重复调用栈 |
| 鼠标 | 工具栏映射、灰色按钮、CommandList 点击、右侧栈帧选择、源码滚轮与滚动条点击/拖动通过 |
| 异步视图 | Asm 打开自动加载，停止后刷新；等待复位等前台命令完成后再查询；失败显示错误且不循环重试 |
| 真实终端 | 安装版通过 ConPTY 点击 Asm、Step、Reset、Run、Reconnect；拖动滚动条可从 tasks.c 首部到末尾 5310 行；CommandList 弹窗可点击打开 |
| STM32 实板 | 安装版完成断点、源码/指令单步、观察点、内存、反汇编、栈、暂停、复位、run、错误清理；显式断开/连接和 reconnect 动作均通过 |
| 汇编显示 | 确认实际 GDB 指令非空，制表符展开为可显示的空格；复位后显示 Reset_Handler 的指令，已消除旧指令短暂回填与重复查询 |
| 通用环境 | ARM / RISC-V / x64 MI 模拟，以及通用服务启动与清理通过 |
| npm | 打包不含 tools，安装/升级、CMD/PowerShell 入口、配置保留、隔离卸载/重装通过；全局安装版本为 0.3.0，EXE SHA256 与构建产物相同 |

原生 EXE 为 1,431,552 字节，比 0.2.0 增加 8 KiB；没有新增运行依赖。本轮未重新测量内存占用。

测试未重新烧写固件。使用现有 `FreeRTOS_Project/Debug/FreeRTOS_Project.elf` 和 STM32F429IG/J-Link 配置。实板日志：`artifacts/hardware-20260916-203738/`；终端 MI 日志、渲染预览：`artifacts/ui-0.3.0/`；npm 验证：`artifacts/npm test 工程 20260916-203708/`。这些生成文件均忽略入库。

复现新增布局预览：设置 `DEBUGTUI_RENDER_DIR` 为输出目录，再执行 `scripts/build.ps1 -Test`。`scripts/test-hardware.ps1` 新增了 reconnect 动作以及反汇编非空/无制表符断言。

## 0.2.0：无参数启动与工程选择

日期：2026-09-16。在环境解耦基础上增加启动配置页，仍使用原生 Rust/Ratatui，无新增运行依赖。

- 16 项单元测试及 Clippy 零警告通过。新增覆盖：CLI 参数值不误判为开关、文件浏览/字段编辑/启动动作、项目相对路径保存、切换工程不携带旧配置、保留会话断点和监视、外部修改冲突检测、中文编辑，以及 45×12 / 80×24 / 120×36 / 180×50 配置页渲染。
- 在真实 ConPTY 终端无参数启动，从空目录打开配置页；键盘选择工程 A，F2 浏览并选择工程目录，自动发现工程内 tools 配置，F5 保存并连接真实 MinGW GDB。运行后停在 main，表达式 counter 返回 1。
- 在同一 TUI 中通过 F2 切换工程 B，旧 GDB PID 22544 退出，新 GDB PID 15888 启动；运行后 main 处 counter 返回 42。最终 Ctrl+Q 退出码 0，无残留调试子进程。
- 失败恢复：本机 MinGW GDB 14.2 对位于含中文路径的测试 EXE 返回 No such file or directory；TUI 显示错误并保留操作能力。通过 F2 修改为可加载的 EXE 路径后连接成功。这是已观察到的该 GDB 路径兼容性限制，不能把配置页支持中文路径等同于所有 GDB 都支持中文程序路径。
- 原有严格 ARM/RISC-V/x86 模拟、真实本机 GDB、STM32/J-Link 实板核心场景、外部 BAT 服务接入、强制终止清理及重连均回归通过。
- npm 安装版直接执行 debugtui，无启动参数时进入配置页，确认没有启动 GDB/Server；在页面选择 STM32 工程后连接实板，p/x g_w25q_jedec_id 返回 0xef4018，F10 源码单步成功，Ctrl+Q 正常清理。参数启动 debugtui --project 工程B 则直接连接到 READY，保持两种入口可用。
- 本轮 npm 安装、升级替换、卸载与重装、便携 ZIP 校验及渲染通过；构建取消 36 ms 退出，未遗留调试进程。

本轮 Release EXE 为 1,423,360 字节（约 1.36 MiB），较前一轮环境解耦构建增加 80 KiB。运行内存未在本轮重新测量。

真实会话日志位于 artifacts/launch ui 工程 20260916-121245/A/logs 和 B/logs。其他回归批次为 environment test 工程 20260916-121306、native-gdb-20260916-121310、hardware-20260916-121502、lifecycle-20260916-121510。配置页操作说明见 README。

## 0.2.0：环境解耦验证

日期：2026-09-16。以下为 Windows x64 版本 0.2.0 的发布前验证。TUI 包与 tools 包已经分离；发行渠道采用 GitHub Release，npm Registry 尚未发布。

| 项目 | 结果 |
|---|---|
| Rust 检查 | 11 项单元测试、Clippy 全目标零警告、Release 构建通过；没有新增第三方 crate |
| 无 tools 运行 | 将 EXE 单独复制到包含中文和空格的目录，严格 MI 模拟会话通过 |
| 目标架构 | ARM、RISC-V、x86-64 三组模拟寄存器全部显示，未发送隐式 monitor/reset/download 命令 |
| 默认 GDB 行为 | local 连接进入 READY；运行使用 -exec-run；未配置 restart/download 时明确拒绝 |
| 服务配置 | 自定义命令可启动；stderr 上无换行的就绪标记能识别；仅清理本程序创建的服务 |
| 真实本机 GDB | MinGW GDB 14.2：启动前设 main 断点、run、表达式、step、x86 寄存器、反汇编、继续和清理均通过；未使用 tools 配置 |
| STM32/J-Link | 通过 tools/debug-env.toml 完成核心调试、源码/指令单步、硬件观察点、内存、栈、reset、run、错误清理及重连 |
| 生命周期 | 外部 BAT 启动服务后接入成功；占用失败保留外部进程；强制结束 TUI 后清理自身子进程；重新连接恢复成功 |
| 构建取消 | 30 秒模拟构建收到 quit 后 15 ms 退出并清理子进程 |
| 分开安装后实板验证 | npm 安装版 TUI + 单独解压到中文/空格目录的 tools，完成全部硬件场景及下载；compare-sections -r 的 6 个只读段全部 matched |
| npm / ZIP | 不含 tools；本地安装、模拟旧版替换、CMD/PowerShell 入口、用户配置保留、卸载及重装通过；便携 ZIP 校验和、版本与渲染通过 |
| 安装版原生程序 | npm 安装版再次通过真实本机 GDB 流程，证明不依赖仓库旁的 tools |

环境解耦首次构建的原生 EXE 为 1,341,440 字节，约 1.28 MiB；当时含文档和许可证的 npm 包约 0.72 MiB，便携 ZIP 约 0.84 MiB。可选 STM32/J-Link 工具 ZIP 约 14.1 MiB，单独打包、单独更新。加入启动配置页后的大小见上方新记录；下方运行资源测量属于历史 0.1.0。

### 复现本次验证

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\test-gdb-environment.ps1
.\scripts\test-native-gdb.ps1 -Gdb C:/MinGW/bin/gdb.exe -Compiler C:/MinGW/bin/gcc.exe
.\scripts\test-hardware.ps1 -Elf path/to/FreeRTOS_Project.elf -Download
.\scripts\test-lifecycle.ps1 -Elf path/to/FreeRTOS_Project.elf
.\scripts\test-build-cancel.ps1
.\scripts\release-assets.ps1 -SkipBuild
.\scripts\test-npm.ps1
.\tools\package.ps1
```

硬件测试脚本从 tools 配置获取服务及连接参数。它专门检查现有 FreeRTOS 固件的符号；不是对任意芯片的通用验收脚本。脚本支持用 -Binary 和 -Tools 分别指定安装版 EXE 与外部工具目录。

本次原始请求、事件、stderr 和 MI 日志保存在忽略入库的 artifacts/ 中，主要批次为 environment test 工程 20260916-094245、native-gdb-20260916-094721、hardware-20260916-094710、lifecycle-20260916-094251、npm test 工程 20260916-094613。

### 当前验证边界

- RISC-V 使用严格 MI 模拟验证，没有连接 RISC-V 实板；OpenOCD 配置是接入示例，本次未执行 OpenOCD 实板测试。
- 已验证 Windows x64 宿主；未验证 Linux/macOS 分发或进程清理。
- 环境解耦阶段 UI 检查覆盖单元测试中的多种终端尺寸及安装包快照渲染；上方记录补充了 0.2.0 启动配置页真实终端操作，下方保留 0.1.0 历史记录。
- detach/resume/disconnect 的最终目标行为受 GDB Server 控制；不会保证跨服务的“退出后保持暂停”。J-Link 的 monitor go 已移到工具配置。
- npm 替换测试使用复用当前 EXE 的 0.0.0-fixture；未在公共 Registry 发布或验证从真实 0.1.0 自动迁移项目配置。

---

# 历史记录：DebugTUI 0.1.0

以下结果保留用于历史对照，其中包内 tools、自动查找及退出策略描述不适用于 0.2.0。

日期：2026-09-16。结果针对 Windows x64、STM32F429IG、J-Link ARM V8（V7.94e Server）、SWD 4000 kHz，以及本机 FreeRTOS_Project 固件。测试使用真实探针和目标板。

## 结果

| 项目 | 结果与证据 |
|---|---|
| Rust 单元测试 | 10 项通过：MI 嵌套/重复字段/转义/异步停止、配置校验与缺省偏好保存、Unicode 输入、4 种终端尺寸、控制台结果保留 |
| Clippy | `cargo clippy --all-targets --locked -- -D warnings` 通过 |
| 发布构建 | `cargo build --release --locked` 通过；原生 EXE 1,323,520 字节 |
| 真实终端 | ConPTY 80×24；连接实板，F5/F6 继续/暂停、F9 断点切换、F10 单步、GDB 表达式、Tab 切换、Ctrl+Q 清理退出通过；命令面板和 Esc 关闭在 demo 中通过；最终 npm 入口在工程目录之外启动并自动找到包内 tools 通过 |
| 普通调试 | 核心 25 项请求及自动退出通过，见下表 |
| 执行控制补充 | `next`、`finish` 返回 Task100ms、控制台 `p/x`、`run` 复位后执行到 main 通过 |
| 下载 | ELF 写入、复位；`compare-sections -r` 的 6 个只读段全部 matched |
| 失败与重连 | 无效表达式使脚本返回非零且跳过后续请求；自动退出清理成功；同一进程断开/重连成功 |
| 外部 Server | 先运行 `tools/start_server.bat`，再 `--connect 127.0.0.1:3333` 成功 |
| 进程所有权 | 探针/端口占用失败不结束外部 Server；强制终止测试 TUI 后，其两个 GDB/Server 子进程均结束；新会话恢复成功 |
| 输入错误 | ELF 缺失、tools 缺失、服务拒绝连接均返回错误，无残留调试子进程 |
| 构建取消 | 模拟 30 秒构建，收到 quit 后 37 ms 退出并结束构建子进程 |
| npm | 本地 tgz 安装、模拟旧版替换为 0.1.0、CMD/PowerShell 入口、配置保留、卸载和重新安装通过 |
| 迁移 | npm 安装目录和 ELF 路径均包含中文与空格；使用安装包内 tools 的实板核心、错误及重连测试通过 |
| 依赖 | 安装后的 11 个 GDB/J-Link 运行资源及许可证哈希与锁文件一致；包不含 Python、Rust 开发缓存或 node_modules |

npm 升级使用 `0.0.0-fixture` 测试包验证替换机制，该 fixture 复用当前原生 EXE，不是一个历史软件版本。未向公共 npm registry 发布，包名 `@debugtui/cli` 尚待正式发布时确定。

## 核心实板请求与结果

| ID | 请求 | 实际结果 |
|---|---|---|
| 1 | connect | 连接成功，MI 异步支持 true |
| 2 | evaluate g_w25q_jedec_id | 15679512，即 0xef4018 |
| 3–5 | 临时断点 DEBUG_PRINTF，continue，wait_stopped | breakpoint-hit，bsp_log.c:51 |
| 6–7 | step，wait_stopped | bsp_log.c:53，PC 0x08000692 |
| 8–9 | stepi，wait_stopped | PC 0x08000694 |
| 10–12 | data_break Log_Tx_En，continue，wait_stopped | watchpoint-trigger，记录 1 → 0 |
| 13 | memory &Log_Tx_En，32 字节 | 返回 0x20000000 起的 32 字节 |
| 14 | disassemble | 返回当前 PC 附近指令 |
| 15 | files | 返回 ELF 调试信息中的源码文件列表 |
| 16–17 | frame 1，frame 0 | 调用栈切换成功 |
| 18 | delete_break（全部） | 会话断点删除成功 |
| 19–20 | continue，pause | 运行中 MI interrupt 成功暂停 |
| 21 | restart | 复位并暂停 |
| 22–24 | 临时断点 main，continue，wait_stopped | main.c:79 |
| 25 | status | STOPPED 快照 |
| 自动退出 | quit | 恢复运行、detach、GDB/Server 结束，退出码 0 |

## 资源测量

真实终端、板子暂停、已有符号与变量快照，采样 5 秒。以下是单次本机测量，不是所有工程的性能承诺。

| 进程 | 私有内存 | 工作集 | 5 秒内 CPU 时间 |
|---|---:|---:|---:|
| debugtui.exe | 2.52 MiB | 8.24 MiB | 15.62 ms |
| arm-none-eabi-gdb.exe | 26.91 MiB | 37.78 MiB | 15.62 ms |
| JLinkGDBServerCL.exe | 56.58 MiB | 58.20 MiB | 156.25 ms |

测量使用最终 npm 安装版，两个监视表达式，包含实际调试操作后的有界 Console 缓冲。运行进程只有原生 TUI、GDB 和 Server，Node 不参与持续调试。工具集约 32 MiB，原生 TUI 约 1.26 MiB，附带完整许可证与文档后 npm 包约 14.7 MiB、解压约 35.6 MiB；精确大小见本机 `artifacts/npm-pack.json`。

原生 EXE 的 PE 导入只有 Windows 系统 DLL：KERNEL32、msvcrt、ntdll、USER32、api-ms-win-core-synch-l1-2-0。无需分发 GCC 或 Rust 运行库 DLL。

## 已修复的问题

1. J-Link Server 就绪提示无换行，按行读取会一直等到超时：改为增量读取并识别就绪文本。
2. 本工具集 GDB 把带引号的 `-target-select` 地址当串口文件：主机名校验后用未加引号的地址参数。
3. 初始 TOML 缺少 watch/breakpoints 字段时保存偏好触发异常：改为表键插入并加入回归测试。
4. 后台异常退出曾可能让 headless 返回成功：现在缺少正常结束事件即返回失败。
5. 极快的停止事件可能被继续命令覆盖成 RUNNING：增加停止代次检查。
6. 进程被强制结束可能留下 GDB/Server：加入 Windows Job Object，仅管理本程序创建的进程。
7. 大量 Server 读寄存器日志会顶掉 GDB 表达式结果：底部 Console 独立保留命令输出。
8. 功能名断点在源码处按 F9 可能重复添加：按文件和行号识别已有断点。
9. 长时间构建期间退出响应慢：退出信号取消构建并清理子进程。
10. 原型的 on_exit=halt 无法保证 J-Link 退出后保持暂停：明确拒绝该值，正常退出使用 resume。

## 复现

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\test-hardware.ps1 -Elf path\to\FreeRTOS_Project.elf -Download
.\scripts\test-lifecycle.ps1 -Elf path\to\FreeRTOS_Project.elf
.\scripts\test-build-cancel.ps1
.\scripts\generate-notices.ps1
.\scripts\package.ps1 -SkipBuild
.\scripts\test-npm.ps1
```

`artifacts/` 保存每次测试的请求 JSONL、全部事件输出、stderr、MI/Server 日志，以及安装包清单和性能测量。原始日志不打进 npm 包。

## 验证边界

- 当前仅验证 Windows x64 和上述 J-Link/STM32 组合；OpenOCD 不在这个最小工具包内。
- 没有做探针物理拔插、休眠恢复、不同板卡或多小时稳定性试验；不把进程终止测试当作 USB 拔插测试。
- 通用变量树、FreeRTOS 专项面板、源码编辑器、多人共享会话尚未实现。
- npm Registry 发布、其他 npm 版本和其他操作系统未验证；本地安装、替换、卸载及程序调试已验证。
- `session.on_exit=resume` 是当前后端支持的退出策略。强制终止后目标状态需要重新连接确认。

## 0.5 SVD 验证

- 单元测试覆盖 STM32F429 的 84 个外设、GPIOB 继承、数组/cluster 展开、位域、大小端和读取副作用。
- UI 通道测试检查收起、隐藏、运行时不读取；展开后只读可见行；同一停止代次不重复读取；手动单项刷新、错误恢复、鼠标命中、滚动条及三档窗口布局。
- 配置页测试覆盖 SVD 文件选择、相对路径保存、清空和无效路径提示。
- `node scripts/test-svd-gdb.cjs --native`：真实本机 GDB 验证 8/16/32 位读取、值变化、错误恢复、运行中拒绝读取，并确认不会覆盖 Memory 页数据。
- `node scripts/test-svd-gdb.cjs --hardware`：使用 tools 配置连接 STM32F429，对比 RCC.CR、GPIOB.MODER、DBGMCU.IDCODE 的外设读取与直接 GDB 表达式结果；不下载或复位固件。
- 其他芯片的 SVD 和硬件行为仍需相应设备验证。自动读取的副作用判断以 SVD 声明为准。

## 0.5.1 输入与补全验证

- UI 测试覆盖输入防抖、候选键盘/鼠标选择、两处输入隔离、添加 Watch 成功/失败、重复回车、F10 保留、过时响应丢弃、运行中不查询、弹窗遮挡和 45×12 至 160×42 的布局。
- `node scripts/test-completion-gdb.cjs`：使用真实本机 GDB，验证命令/子命令/参数补全、全局与静态变量、排除函数名、结构体字段、64 项上限、Watch 添加和运行/暂停后的补全恢复；确认补全不改变变量值、PC 或停止代次。
- `node scripts/test-completion-gdb.cjs target/release/debugtui.exe PATH/FreeRTOS_Project.elf`：使用 tools 中 ARM GDB 离线加载 ELF，验证 `uxCurrentNumberOfTasks`、`xTickCount`、`p/x` 参数等候选，无需连接开发板。
- 彩色 UI 缓冲区可通过 `DEBUGTUI_RENDER_DIR` 环境变量导出。测试记录在 `artifacts/completion-*`，不进入安装包。

## 0.6.0 effects, warm theme and per-item radix

- 61 unit tests plus the real shell-task integration test cover configuration merging, independent variable/register/field/byte formats, exact signed/unsigned 128-bit conversion, unsupported natural values, keyboard and right-click routing, popup clipping and input focus.
- Effects tests verify actual connection phases, confirmed stop events, real source traces, bounded caches, idle scheduling, focus loss, Off mode and persistent task outcomes. Color-buffer previews cover 45×12, 80×24 and wide layouts, numeric/appearance menus and sampled animation frames.
- Native GDB integration saves all three motion modes and individual formats: zero additional MI commands, unchanged watched value, PC, generation and STOPPED state. FreeRTOS ARM ELF symbol completion is also exercised offline; the SVD native-memory read suite is rerun. These checks do not certify a physical R52/STM32 target.
- Real terminal smoke: connect native GDB, breakpoint main, run to breakpoint, switch register format from hex to decimal, use Appearance and exit cleanly. A fast Tab→f race discovered here was fixed by deriving the keyboard selection from current data before the next draw.
- Runtime sample (native GDB stopped, Full mode, one terminal): TUI working set 7.46 MiB, private bytes 1.73 MiB, CPU 0.03125 seconds over 3.046 seconds. This is a local sample, not a performance guarantee.
- npm fixture checks install, upgrade, CMD/PowerShell shims, configuration preservation, isolated uninstall and reinstall.

Artifacts: `artifacts/ui-0.6.0/`, `artifacts/completion-native-1789607444647/`, `artifacts/completion-arm-elf-1789607447807/`, `artifacts/svd-native-1789607448656/`.

## 0.6.1 Console 历史与会话时间戳

- `cargo test --locked`：67 项单元测试 + 1 项实际 shell 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 通过。
- Console 新增测试覆盖滚轮、轨道点击、滑块拖动至两端、历史期间追加输出、2,000 行缓存淘汰后的记录锚点、窄窗口缩放、弹窗隔离、Latest / End 恢复跟随、输入草稿保留和实际命令分发。浏览历史不发送调试请求。
- `scripts/test-logs-gdb.cjs` 使用真实本机 GDB 与独立 C 测试程序，执行断点、Run、Next、错误命令、多行 printf、Reconnect、Disconnect / Connect。14 个请求生成 3 份日志（114 / 24 / 24 行），检查旧文件字节不变、全部物理行带时间戳、161 条 log 事件携带时间字段且原始文本保持不变。记录：`artifacts/logs-native-1789609065500/verification.json`。该测试不连接物理 MCU。
- 时间测试覆盖 Windows 时钟格式、UTC 闰日、同毫秒文件名碰撞不覆盖、毫秒会话时长和多行/空行逐行前缀。文件通过 `create_new` 排他创建。
- UI 预览由真实 Ratatui 缓冲导出：`artifacts/ui-0.6.1/console-live.png`、`console-history.png`、`console-history-narrow.png`。保留底部输入框，Console 右边是独立轨道，浏览状态与新消息计数在标题行。
- 本地安装 0.6.1 后，以真实终端连接本机 GDB：多行 printf 输出 35 条记录，Shift+PgUp / Home 浏览首条记录；保持历史视口执行新命令，Latest 增加 3 条记录且旧内容保留；End 恢复实时跟随，Ctrl+Q 退出码 0。日志位于 `artifacts/ui-0.6.1/terminal/`。原生命令/变量补全回归同样通过：`artifacts/completion-native-1789609102557/verification.json`。
- npm 独立安装、升级、卸载和重新安装均通过；本机安装的可执行文件 SHA-256 与 release 构建相同。

## 0.7.2 多核健壮性与单核回归（2026-09-18）

- 89 项 Rust 单元测试 + 1 项实际 shell 集成测试通过；Clippy `--all-targets -- -D warnings` 通过。新增覆盖 ELF32/64 正确偏移、截断/越界、TCL 完整响应/错误/超时、Live Watch 断线重连与 target-specific 读取、并发分核偏好保存、启动页合并和切核视图缓存失效。
- `scripts/test-multicore-gdb.cjs target/release/debugtui.exe`：两个真实本机 GDB，稳定核序号、按序连接、立即切换快照、不同 Watch/断点保存与恢复、Run、单步、重连、Build 全流程通过。另测第二核连接失败回滚、后台核 Run 失败上报、单个显式 core 的共享服务、服务启动失败清理，以及多核 Build 中通过 Console quit 退出取消。最终记录：`artifacts/multicore-1789740186149/verification.json`。
- 无 `[[cores]]` 的旧单核工程仍直接进入原 Session；真实 GDB connect/break/run/step/quit、Watch 增删与命令/符号补全通过。单核 Snapshot 不增加 core 字段。
- ARM / RISC-V / x64 的严格 MI 模拟服务通过：寄存器来自 GDB，不注入芯片命令；本地 READY、未配置动作报错、无换行 stderr 就绪消息和自有进程清理通过。
- Pause 五种情况通过：缺失停止事件、已停止错误、延迟事件、补发中断，以及目标确实仍运行时有界超时。脚本使用 PowerShell 7 (`pwsh`) 执行；Windows PowerShell 5 会将预期的 stderr 负例提前变成终止错误。
- 会话日志回归通过：14 个请求、3 份独立日志、全部物理行带时间戳、旧日志字节不变。记录：`artifacts/logs-native-1789739873974/verification.json`。
- 这些验证不连接物理 MCU。真实多核芯片的 CTI 联动、独立核暂停能力、AP 访问与缓存一致性仍需对应硬件验收；当前共用 ELF/GDB/SVD，不视为异构多镜像支持。

### 2026-09-19 补充回归与收尾

- 复现并修复：重复 Connect 原本进入失败回滚、拆掉现有多核连接；现在在派发前拒绝，READY/STOPPED 会话保持有效。
- 多核 `set_elf` 明确拒绝单核局部切换，引导通过 F2 Setup 更换共享 ELF 并整体启动。单核 `:elf` 保留；界面只在后端确认成功后更新路径，失败或发送失败保持原路径。
- 单核/多核首次 Run 前的断点新增、恢复及 Console 删除均验证持久化。退出前读取实际 GDB 断点列表，避免 READY 状态的变更遗漏；读取失败保留之前已保存列表。
- TCL 同步初始化在每条命令前检查退出取消；退出时不再执行余下配置命令。共享服务就绪标记跨读取块且包含空格、无换行时仍可识别；退出后验证自有服务 PID 已消失。
- 90 项单元测试 + 1 项实际 shell 集成测试、Clippy、release 构建通过。最终 release 双 GDB 脚本 9 组场景通过：`artifacts/multicore-1789819904726/verification.json`。
- 单核真实 GDB Watch/补全：`artifacts/completion-native-1789819906319/`；日志轮转/时间戳：`artifacts/logs-native-1789819909213/verification.json`。ARM/RISC-V/x64 模拟寄存器与五种 Pause 场景回归通过。

### 2026-09-19 Watch 补全、结构体与地址表达式

- 96 项 Rust 单元测试 + 1 项实际 shell 集成测试通过，Clippy `--all-targets -- -D warnings` 和 release 构建通过。树形 UI 新增回归：鼠标展开、Enter/左右键、成员独立进制、折叠后的选中项、数组分页、滚动后关闭顶层项、禁止从成员行误删其他观察项、运行时展开限制和窄窗口点击区域。
- `node scripts/test-watch-tree-gdb.cjs target/release/debugtui.exe`：真实本机 GDB 验证全局/静态符号、`.` / `->` / 嵌套成员和强转成员补全；结构体展开、数组 32/64/70 项分页、成员变化刷新；结构体/整数的地址强转和解引用；无效地址、未知类型、失效路径；256 节点与 8 层限制；折叠/删除零 MI，以及所有临时变量对象释放。记录：`artifacts/watch-tree-1789821399632/verification.json`。
- 修复 GDB 对不可读地址仍成功创建变量对象、却返回空 value 的边界情况：取得读取诊断并标记错误，避免显示为有效类型。折叠后重开数组从首批 32 项开始，避免一次读回所有历史分页。
- `scripts/test-completion-gdb.cjs` 原有单核补全、Watch 增删和零目标写入断言通过：`artifacts/completion-native-1789821345670/verification.json`。使用工程 `Debug/FreeRTOS_Project.elf` 和 tools 内 ARM GDB 的离线全局变量补全通过：`artifacts/completion-arm-elf-1789821348389/verification.json`。
- 双真实 GDB 的 10 组回归通过，新增同名结构体在两核上的数值（91/41）、展开状态与删除隔离验证：`artifacts/multicore-1789821345691/verification.json`。无 `[[cores]]` 的单核路径保留。
- ARM / RISC-V / x64 模拟 MI、通用服务启动/回收、五种 Pause 场景通过：`artifacts/environment test 工程 20260919-203549/`、`artifacts/pause-20260919-203556/`。
- 从实际 Ratatui 缓冲导出并检查 Watch 树预览：`artifacts/watch-tree-ui/watch-tree.png`、`watch-tree-compact.png`。本轮真实目标求值在本机测试程序中完成，ARM ELF 补全不连接硬件；未做物理 MCU 内存访问验证。
- npm 本地包重新安装成功，`debugtui --version` 为 0.7.2。安装目录中的 exe 与 release 构建 SHA-256 相同：`32EFDEC1FCE8D3FBE4708080C50D62012ECDE88B10829C913ACAC60395F0CD4B`；直接用已安装 exe 重跑 Watch 树测试通过：`artifacts/watch-tree-1789821508317/verification.json`。
