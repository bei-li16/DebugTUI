# STM32F429 系统寄存器描述文件

DebugTUI 从 1.1.5 起通过 npm 和完整 ZIP 提供 `profiles/registers/stm32f429.toml`。它与仓库 `reference/stm32f429-system-registers.toml` 字节一致，继承同目录的 `cortex-m4.toml` 和 `armv7m-common.toml`，展开后包含 **617 项寄存器定义、1464 项普通/读取位域和 32 项写入载荷位域**。计数包含别名和架构容量位置，不等于芯片上独立实现的寄存器数量。

## 在现有工程启用

目录现提供只读 ETM/TPIU discovery：先核对 M4 CPUID 及 ROM 表项 PRESENT/FORMAT/offset，再读取受 TRCENA 保护的 CID/PID/DEVTYPE。TPIU 需要匹配 Arm part 0x9A1 及 M4 DEVID；ETM 需要匹配 part 0x925，并进一步核对 ETMv3.5 IDR、CCR、CCER 和 PDSR 的供电证据。Host 不包含 F429 地址或芯片识别分支。判据来自 Arm 100166_0001、DDI0440C 和 RM0090，并保存在此芯片 TOML 的 discovery 规则中。

识别成功后只读 ITCTRL，以区分未知模式和明确未使能模式；不写 TRCENA、ETMCR、ITCTRL 或锁寄存器。ETM LAR 保留 `description_unsupported`；MVFR2、DWT PCSR、SCS PID5/6/7、ITM 软件锁等未获证明的项继续保留未知说明。SysTick CTRL、DHCSR、DWT FUNCTION 和 FPSCR 的自动读取保护、NVIC 容量边界和 FIFO/集成模式条件仍有效。

**1.2.0 自动加载：**选择随包的 `stm32f429` 芯片，CPU registers 保持 **Automatic**，在工程和 profile 没有显式 CPU/catalogue 覆盖时，芯片入口会引用安装目录的 F429 描述，并提供 PPB 和 DBGMCU 的 `core-tcl` 访问映射。已显式选择 `cortex-m4` 或自定义 catalogue 的工程仍保留原选择；可在 Setup 恢复 Automatic。详细优先级、用户芯片覆盖和缺失默认目录处理见 [Host 与芯片描述](chip-description-host.md)。

以下显式配置方式适用于需要固定目录副本的工程，也兼容 1.1.5：

先找到完整安装包中的 `profiles/registers/`。便携版位于解压目录；npm 安装位置可在 PowerShell 查询：

```powershell
$debugtuiPackage = Join-Path (npm.cmd root -g) '@debugtui/cli'
Join-Path $debugtuiPackage 'profiles/registers'
```

需要工程自管副本时，将该目录中的 `stm32f429.toml`、`cortex-m4.toml`、`armv7m-common.toml` 三个文件一起复制到工程的 `registers/` 目录。在现有 `debug.toml` 的 `[registers]` 中合并以下两项，保留工程自己的探针、ELF、端口和访问通道配置，不重复创建已有表：

```toml
[registers]
cpu = "cortex-m4"
catalogue = "registers/stm32f429.toml"
```

也可将 `catalogue` 指向安装包内该文件的绝对路径。路径相对声明它的配置文件解析；独立 EXE 下载者需要另外获取上述三份 TOML，或使用完整 ZIP/npm 包。已有多核配置应在对应 `[cores.registers]` 设置目录，核对其覆盖关系。

打开 System Regs 的 Status，检查目录来源和继承链。显式 F429 目录提供芯片参考描述；只设置 `cpu = "cortex-m4"` 仍使用通用 Cortex-M4 目录。升级保留工程副本和用户自定义文件，需要更新副本时先比较自己的修改。

## 修复范围与访问限制

- 补正权限和读写规则，包括 PPB 特权访问、CONTROL/FAULTMASK、DHCSR、SHCSR、MPU 更新同步、故障地址有效性、ETM 事件与 Trace ID、组件标识和相关勘误条件。
- 保留证据限制：组件跨页标识的实现/版本差异，以及 DWT FUNCTION 字段在所引架构文档中的矛盾条款；未确定的行为仍有明确说明。
- RW 权限描述不会自动授予 writer；当前目录只继承 R0–R12、SP、LR、PC 的 16 个既有 GDB writer。写入预览的验证范围仍以实际后端回执为准。
- 不支持读取的寄存器及其别名不会进入自动读取候选；可选组件、实际容量、启用状态和当前目标身份继续由已有资格检查控制。手动读取也不能绕过缺失的读取后端。
- 本版的审计与发布验证为软件验证，没有新增实板验收。目录数量和文档可信度不能证明当前芯片全部实现或可访问。

详细审计记录见仓库 `reference/stm32f429-system-registers-fix-followup.md` 和同名 JSON；原手册及完整审计材料保存在源码仓库，不进入运行包。
