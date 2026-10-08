# 内置 ARM 调试工具

本目录供 STM32F429 与 THA6104 / THA6206 / THA6412 共用，通过 npm 包和便携 ZIP 分发。1.1.1 在 1.0.0 的 R52 模板基础上补齐 THA 板级配置与 SVD；旧版 1.0.0 安装包不含这些新增配置。

## 目录内容

```text
tools/
  debug-env.toml   公共 GDB/OpenOCD 环境
  debug.toml       新工程的最简配置模板
  bin/            GDB、OpenOCD、配套脚本及许可证
  devices/        STM32、THA 芯片描述与共用设置
  openocd/        STM32/THA 板级配置（含 THA 复位过程）、其他 R52 板卡模板、探针配置
  svd/            STM32F429、THA6104/6206/6412 外设描述及来源说明
```

## 安装与使用

安装 DebugTUI npm 包或解压完整便携 ZIP 后，在固件工程目录运行 `debugtui`。程序优先读取当前目录的 `debug.toml`；没有时查找其他调试工程 TOML，一个则加载，多个则显示列表；均不存在时自动创建最简 `debug.toml`。仅创建配置可运行 `debugtui --init-project`。

```toml
[tools]
profile = "builtin:arm-openocd"
probe = "cmsis-dap"
```

在 Setup 中选择 Probe、Chip、Debug cores 和 ELF。Probe 支持 `cmsis-dap`、`jlink`、`stlink`，空值继承环境默认值。工具从 DebugTUI 安装位置读取，无需复制到工程 `.vscode`，也不需要在工程内记录安装目录。ELF、源码、日志和显式配置文件路径仍相对于工程 TOML。

Tools / profile 和 SVD file 按 **F2** 可选择包内文件或浏览外部文件，列表显示实际安装路径。内置 SVD 保存为 `builtin:svd/STM32F429.svd`；选择 Automatic 则跟随 Chip。旧 profile 下切换 Probe 会打开兼容工具选择列表；选择内置 profile 后应用探针，Esc 保留原配置。切换时只迁移内容完全一致的旧版原始 SVD，自定义文件保留。

内置文件随软件升级更新。自定义芯片 TOML 可在 Setup 的 **Chip config** 中选择，或放到 DebugTUI 用户目录的 `profiles/chips/<chip>.toml`；优先级为工程指定文件、用户文件、内置文件。自定义文件可用 `extends = "builtin:devices/tha6206.toml"` 继承默认配置，再引用同目录的板级 cfg/SVD。三款 THA 已有参考工程对应的板级参数；其他 R52 板卡仍须提供自己的配置。

## THA 芯片

配置参考本地 `mcal-vsconfig`（提交 `4a24a8df28becd097bb60451c7583fa22da7738c`）的 `openocd-debugtui.cfg`、`gdb.cfg` 及三个芯片目录。

| Chip | 可选物理核 | GDB 端口 | 自动 SVD |
| --- | --- | --- | --- |
| tha6104 | 0 | 3333 | THA6104.svd |
| tha6206 | 0、1 | 3333、3334 | THA6206.svd |
| tha6412 | 0、1、2、3 | 3333～3336 | THA6412.svd |

仅所选核开放 GDB 端口，物理编号不重排：单选 core1 仍用 3334，选择 `[1,3]` 用 3334、3336。AP1 提供 CPU/CTI 调试和 `APB_1` 直接访问；AP3 提供 `AHB_3` 直接访问，用于内存与运行中 Watch。Tcl 端口为 6666，所有服务只监听本机，Telnet 关闭。默认 SWD 5 MHz，探针由 Setup 选择。THA6206 已使用 CMSIS-DAP/SWD 实板验证；其他探针、JTAG、THA6104/6412 目前仅做离线验证。

GDB 使用 `armv8-r`，沿用参考工程的 GHS DWARF unwind 设置，并把 `0x08000000`～`0x08600000`（末地址不含）标为只读 Flash 区域，让 GDB 自动选择硬件断点。此内存窗口不构成烧录算法。

`devices/families/tha6.toml` 区分 `DEBUGCORE`（所选 GDB 核）和 `EXAMINECORE`（需要初始化调试通道的核）。默认 examine 全部物理核，以便 MCAL 启动辅助核；连接时只 halt 所选核，不自动复位或 resume 其他核。其他固件若要求未选核不 examine，可通过自定义 `service.args` 将 `EXAMINECORE` 改为 `${core_mask}`。数组整体覆盖，须保留两个 `-s` 搜索目录、Probe、Chip 和核掩码参数。

THA 的 `chipreset` 是芯片级复位，由 core0 执行一次；不含 core0 的选择停用此按钮。复位脚本恢复调试模块后重新 examine，刷新状态并带超时 halt，最后释放 reset catch；成功或失败均尝试恢复原 target，失败保留错误并允许下次重试。EDRCR.CSE 使用 bit2（`0x4`），没有复制旧配置中先 resume 再复位的流程。THA6206 的 core0-only 与双核已分别连续复位三次并到达 `_main`；故障分支另有 Tcl 注入测试。首次调试模块复位可能让 MEM-AP 写事务报错；脚本保留提示并检查所有所需 CPU 恢复，不能仅凭该提示判断芯片复位失败。

2026-10-08 实板观察到 THA6206 两个核的外部 MIDR 均为 `0x410FD161`（Cortex-R52+），其芯片配置明确选择 `cortex-r52+` 寄存器目录，覆盖旧用户芯片目录中 R52 的默认关联。GDB 的 R0～R12、SP、LR、PC、CPSR 正常可读。包内注入式 CP15、banked、VFP、timer、PMU、GIC 适配器只验证了 R52（部件号 `0xD13`），尚未适配 R52+（`0xD16`），THA6206 默认不启用它们；这些扩展项显示不支持，不应通过修改身份校验或通用 MRC 回退绕过。APB/AHB 内存访问和外设 SVD 不受此限制。

### MCAL 工程

`openocd/tha6.cfg` 同时包含板级 target 定义和 `dbgreset` / `chipreset` 过程；加载配置只注册复位过程，显式执行复位命令时才操作硬件，无需额外复位脚本文件。

把 [tha6-bundled-project.toml.example](../profiles/tha6-bundled-project.toml.example) 复制到工程根作为 `debug.toml`，核对 ELF 与 Build / Download 路径后运行 `debugtui --setup`。该示例使用包内 GDB、OpenOCD 和 SVD，工程无需再携带这三项资源。编译仍需工程对应的 GHS 环境；下载仍调用 `.vscode/Load.bat` 和其中的厂商 `jtag.exe`，未声明 OpenOCD 的 THA Flash 算法，也未使用通用 `-target-download` 代替厂商烧录。

示例按 MCAL 启动屏障安排：所选辅助核先连接，core0 最后连接并复位一次，再 resume 未选辅助核。core0-only、双核、四核均复用这份文件；core1-only 或其他不含 core0 的组合是附加调试，要求 core0 已完成初始化。运行中 Watch 使用 `AHB_3`；缓存中的数据能否被 MEM-AP 观察仍由固件内存属性决定。Bao 等固件使用最简工程配置，并填写自身的 ELF、构建、下载和启动策略。

自定义 JTAG 板卡可在 `service.args` 加入 `-c`, `set THA_TRANSPORT jtag`；降速可加入 `-c`, `set THA_ADAPTER_KHZ 1000`，均放在 `-f .../tha6.cfg` 之前。默认 SWD DPIDR 为 `0x6BA02477`，JTAG 使用参考配置注明的 `0x6BA00477`。

离线验证入口：`node scripts/test-tha-tools.cjs`（所有非空选核组合、探针/传输配置、非法掩码和复位故障处理）；`cargo test --locked --lib devices::tests`（配置继承与 MCAL 策略）；`cargo test --locked --lib bundled_tha`（三个 SVD 的真实解析）。这些检查不连接探针，不代表三款芯片的实板回归通过。

THA6206/MCAL 实板入口：`node scripts/test-tha-bundled-hardware.cjs --run --binary EXE --project-root MCAL_ROOT`；额外传入 `--build-download` 会通过工程任务实际编译和烧录。工程须已有上述示例对应的 `debug-builtin.toml`，并独占调试器。报告保存在 `artifacts/tha-bundled-hardware-*/report.json`，同时记录工具哈希、三个选核组合的协议日志、真实断点/单步、Scope Core/All、断点转多核与联停、Watch/实时 AHB 采样、内存、重连和复位。扩展寄存器测试验证明确拒绝及无注入，不代表其读取功能已通过。

仓库中的 `install.ps1` 仅供旧版工程复制工具到 `.vscode` 的兼容用法，不随 npm 包分发。新工程直接使用内置工具。自定义配置的完整示例见 [用户指南](../USER_GUIDE.md#内置工具与自定义芯片配置)。
