# 可选调试环境

此目录独立维护 GDB、OpenOCD、板级配置与许可证。DebugTUI 的核心和安装包不依赖此目录结构，也不会自动寻找它；工程通过 `[tools] profile`、`--tools-dir` 或 `--environment` 引用。工程目录下有 `tools/debug-env.toml` 且没有 `[tools]` / `[gdb]` 时，Setup 自动选用它。

| profile | 探针 | 板级配置 |
|---|---|---|
| `debug-env.toml` | J-Link，经 OpenOCD jlink 驱动 | `config/stm32f429-live.cfg` |
| `debug-env-cmsis-dap.toml` | CMSIS-DAP | `config/stm32f429-dap.cfg` |
| `debug-env-stlink.toml` | ST-Link V2J24 及以上或 V3，尚未上板验证 | `config/stm32f429-stlink.cfg` |

三者都使用 `bin/gdb` 的 ARM GDB 和 `bin/openocd` 的 OpenOCD；GDB 3333、TCL 6666 仅监听本机，telnet 关闭。GDB/Server 路径按 TOML 所在目录解析，`${profile_dir}` 用于环境自己的资源路径。复制 profile 可以建立不同板卡的配置；修改端口时同步修改板级 cfg 和 `target.endpoint`。TUI 无需重新编译。

每个 profile 写明用哪个 GDB、如何启动 OpenOCD 和连接、复位/运行/下载/断开时发送的命令，以及 Live refresh 使用的内存通道；三者只有板级配置（探针）不同。工程的 `debug.toml` 用 `[tools] profile` 选其中一个。`dependencies.lock.json` 记录 `bin/` 下每个文件的大小和 SHA-256，打包和安装前据此校验，DebugTUI 运行时不读它。

## 安装到工程

~~~bat
tools\install.bat <工程目录> [auto|jlink|cmsis-dap|stlink] [ELF]
~~~

把 profile、板级配置、`bin/` 和校验清单按本目录的结构复制到 `<工程>\.vscode\`，profile 里的相对路径不用改；在仓库中运行时还会把 STM32F429 的 SVD 复制到 `.vscode\svd\`。复制前后都按校验清单检查文件。

- 探针：省略或 `auto` 时按已连接的 USB 设备识别 J-Link、ST-Link 或 CMSIS-DAP；识别不到或有多种时用 J-Link（`debug-env.toml`）。
- `debug.toml`：工程里没有时新建，写入 profile、ELF、SVD 和 `on_exit`；没指定 ELF 时取工程里最新的 `*.elf`。已有时只改 `[tools] profile`（指定了 ELF 时也改 `[program] elf`），原文件先存为 `debug.toml.bak`；带自己 `[gdb]` 的不修改，只给出提示。
- 重复运行：按上次安装的校验清单删除这一版已不再包含的文件；你改过的 profile 或板级配置先存为 `.bak`。
- 在 cmd 中路径可以用双引号或单引号。

安装后在工程目录运行 `debugtui`，Setup 已带出该工程，按 F5 开始调试；`debugtui --project .` 则直接连接。`.vscode\bin` 约 15 MB，不想提交到 git 时在工程的 `.gitignore` 加 `.vscode/bin/`。

## OpenOCD 版本

`bin/openocd` 是用 [openocd-adapter](../third_party/openocd-adapter/README.md) 补丁构建的 OpenOCD：上游 `d3ebb8d` 加 DebugTUI 的 ARMv8-R 寄存器适配，版本串 `0.12.0+dev-gd3ebb8d-dirty`。它保留上游的全部调试功能，另外提供 R52 寄存器专用协议（`aarch64 r52_read`、`banked`、`timer`、`vfp` 等），因此 STM32 与 R52 工程使用同一个 OpenOCD。程序与 `third_party/openocd-adapter/source.lock.json` 记录的 Windows 候选摘要一致；该构建只做过软件验证（构建、协议命令和配置解析），没有连接探针或板卡。来源和许可见 `bin/openocd/PROVENANCE.txt` 与 `licenses/`，GPL 对应源码 `corresponding-source.zip` 随 [v0.10.0-readonly.1](https://github.com/bei-li16/DebugTUI/releases/tag/v0.10.0-readonly.1) Release 提供。

## R52 多核芯片

THA6206 等 R52 工程可以在其环境 profile 中把 `service.command` 指向本目录的 `bin/openocd/bin/openocd.exe`，并用 `-s` 指向 `bin/openocd/scripts`，继续使用工程自己的板级 `openocd.cfg`（核心 target、AP、CTI、复位脚本）。寄存器专用读取按 [只读寄存器指南](../docs/registers-readonly-guide.md) 在 `[registers]` 中显式配置。板级 cfg 若依赖厂商 OpenOCD 的专用命令或脚本，先用下面的命令检查，它在不连接探针的情况下解析配置并报告缺失的命令或文件：

~~~powershell
./tools/bin/openocd/bin/openocd.exe -s ./tools/bin/openocd/scripts -f <工程>/.vscode/openocd.cfg -c shutdown
~~~

## OpenOCD 脚本

`bin/openocd/scripts` 只保留随附 profile 和 STM32 系列需要的 40 个上游文件：30 个 `target/stm32*.cfg`，J-Link、CMSIS-DAP、ST-Link 接口，以及 `mem_helper.tcl`、`target/swj-dp.tcl` 等通用辅助脚本，内容未修改。其他芯片的 target/board 脚本从对应 OpenOCD 版本或芯片厂商获取，放到工程目录并用 `-s` 或 `-f` 引用；OpenOCD 在解析配置时就会报告缺失的文件，不会拖到连接之后。

## STM32F429 / J-Link

`debug-env.toml` 使用 J-Link 探针、SWD 1000 kHz；`config/stm32f429-live.cfg` 同时建立 M4 CPU target 和 AP0 `mem_ap` target。

~~~powershell
debugtui --tools-dir ./tools --elf ./build/app.elf
~~~

F2 选择该 profile，Watch/外设右键 → Memory access / Live refresh，即可选择默认 GDB、暂停时 Core target 或运行时 AHB MEM-AP 0。打开实时刷新前，Watch 需要在暂停时完成地址解析。无需 Python，也没有增加 TUI 常驻进程。多核板卡可参考 [docs/examples/multicore-access.md](../docs/examples/multicore-access.md)，不能照搬 F429 的 AP0 配置。

OpenOCD 通过 libusb 访问 J-Link。Windows 上该探针接口需要绑定 WinUSB 兼容驱动；绑定后 SEGGER 自己的软件可能无法打开探针，两者切换时需要改驱动，本机记录见用户手册。

## STM32F429 / CMSIS-DAP

`debug-env-cmsis-dap.toml` 使用 CMSIS-DAP 探针、SWD 1000 kHz；CPU 与运行时 AHB 通道由 `config/stm32f429-dap.cfg` 提供，端口与 J-Link profile 相同。

~~~powershell
debugtui --project <工程目录> --environment ./tools/debug-env-cmsis-dap.toml
~~~

也可在启动页选择该 Tools / profile 并保存。换探针不需要重新编译 TUI。若工程原先配置了启动 J-Link 的 Download command，请清空它，使用 profile 中的 GDB 下载动作；Build command 仍依赖工程自己的编译工具链。

## STM32F429 / ST-Link

`debug-env-stlink.toml` 通过 OpenOCD 的 st-link 驱动（dapdirect 模式）使用 ST-Link，SWD 1000 kHz，`config/stm32f429-stlink.cfg` 与其他两个 profile 一样建立 M4 CPU target 和 AP0 `mem_ap` target。V2J24 之前的 ST-LINK/V2 固件不支持该模式，需要先用 ST 的工具升级。此 profile 只经过配置解析检查，尚未在板卡上验证。

## SEGGER J-Link GDB Server（不随附）

此目录不包含 SEGGER J-Link 软件：其许可证要求每次再分发都事先取得 SEGGER 书面授权。需要 SEGGER Server 时，从 SEGGER 官网安装 J-Link Software，把 [docs/examples/jlink-gdb-server.toml](../docs/examples/jlink-gdb-server.toml) 复制到本目录，并把 `service.command` 改为本机安装路径。

该 Server 没有 OpenOCD 的 TCL 端口，Memory access / Live refresh、寄存器专用读取和多核 CTI 同步都不可用；本地曾记录约 52–53 秒后长会话访问失效，使用前阅读用户手册的对应案例。

## 手动启动外部服务

~~~powershell
./tools/bin/openocd/bin/openocd.exe -s ./tools/bin/openocd/scripts -f ./tools/config/stm32f429-live.cfg
# 在另一个终端中运行：
debugtui --environment ./tools/debug-env.toml --connect 127.0.0.1:3333 --elf ./build/app.elf
~~~

`--connect` 禁止 TUI 启动配置中的 service，已有服务归调用者管理。OpenOCD 在 GDB 断开后继续运行，用 Ctrl+C 结束。

默认配置在退出前发送 `monitor resume`，再 detach，目标继续运行。若需要保持暂停，不要退出该环境会话；其他服务可使用不同的 `before_disconnect` 与 `session.on_exit` 策略。


## 独立打包

~~~powershell
./tools/package.ps1
~~~

生成 `artifacts/debugtui-tools-arm-win-x64.zip`，解压后得到 tools 目录。包中只有运行所需的 profile、板级配置、安装脚本和 `bin/`；随附 OpenOCD 的补丁源码在 `third_party/openocd-adapter/`，不进入该包。打包前校验 `dependencies.lock.json`，`bin/` 下有清单外的文件时拒绝打包；`bin/` 下的文件变化后运行 `./tools/package.ps1 -UpdateLock` 重新生成该清单。TUI 版本升级无需重复安装工具集。

此最小工具集无 Python。GDB 的许可证位于 `bin/gdb/license.txt`；OpenOCD 的 GPLv2 文本、其他组件许可和来源在 `bin/openocd/COPYING.txt`、`bin/openocd/licenses/`、`bin/openocd/PROVENANCE.txt`。应用源码许可证不替代第三方条款。
