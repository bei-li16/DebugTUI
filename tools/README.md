# 可选调试环境

此目录独立维护 GDB、OpenOCD、板级配置与许可证。DebugTUI 的核心和安装包不依赖此目录结构，也不会自动寻找它；工程通过 `[tools] profile`、`--tools-dir` 或 `--environment` 引用。工程目录下有 `tools/debug-env.toml` 且没有 `[tools]` / `[gdb]` 时，Setup 自动选用它。

| profile | 探针 | 板级配置 |
|---|---|---|
| `debug-env.toml` | J-Link，经 OpenOCD jlink 驱动 | `config/stm32f429-live.cfg` |
| `debug-env-cmsis-dap.toml` | CMSIS-DAP | `config/stm32f429-dap.cfg` |

两者都使用 `bin/gdb` 的 ARM GDB 和 `bin/openocd` 的 OpenOCD；GDB 3333、TCL 6666 仅监听本机，telnet 关闭。GDB/Server 路径按 TOML 所在目录解析，`${profile_dir}` 用于环境自己的资源路径。复制 profile 可以建立不同板卡的配置；修改端口时同步修改板级 cfg 和 `target.endpoint`。TUI 无需重新编译。

## OpenOCD 脚本

`bin/openocd/scripts` 只保留随附 profile 和 STM32 系列需要的 36 个上游文件：26 个 `target/stm32*.cfg`，J-Link、CMSIS-DAP、ST-Link 接口，以及 `mem_helper.tcl`、`target/swj-dp.tcl` 等通用辅助脚本，内容未修改。其他芯片的 target/board 脚本从对应 OpenOCD 版本或芯片厂商获取，放到工程目录并用 `-s` 或 `-f` 引用；OpenOCD 在解析配置时就会报告缺失的文件，不会拖到连接之后。

## STM32F429 / J-Link

`debug-env.toml` 使用 J-Link 探针、SWD 1000 kHz；`config/stm32f429-live.cfg` 同时建立 M4 CPU target 和 AP0 `mem_ap` target。

~~~powershell
debugtui --tools-dir ./tools --elf ./build/app.elf
~~~

F2 选择该 profile，Watch/外设右键 → Memory access / Live refresh，即可选择默认 GDB、暂停时 Core target 或运行时 AHB MEM-AP 0。打开实时刷新前，Watch 需要在暂停时完成地址解析。无需 Python，也没有增加 TUI 常驻进程。多核板卡可参考 [examples/multicore-access.md](examples/multicore-access.md)，不能照搬 F429 的 AP0 配置。

OpenOCD 通过 libusb 访问 J-Link。Windows 上该探针接口需要绑定 WinUSB 兼容驱动；绑定后 SEGGER 自己的软件可能无法打开探针，两者切换时需要改驱动，本机记录见用户手册。

## STM32F429 / CMSIS-DAP

`debug-env-cmsis-dap.toml` 使用 CMSIS-DAP 探针、SWD 1000 kHz；CPU 与运行时 AHB 通道由 `config/stm32f429-dap.cfg` 提供，端口与 J-Link profile 相同。

~~~powershell
debugtui --project <工程目录> --environment ./tools/debug-env-cmsis-dap.toml
~~~

也可在启动页选择该 Tools / profile 并保存。换探针不需要重新编译 TUI。若工程原先配置了启动 J-Link 的 Download command，请清空它，使用 profile 中的 GDB 下载动作；Build command 仍依赖工程自己的编译工具链。

## SEGGER J-Link GDB Server（不随附）

此目录不包含 SEGGER J-Link 软件：其许可证要求每次再分发都事先取得 SEGGER 书面授权。需要 SEGGER Server 时，从 SEGGER 官网安装 J-Link Software，把 [examples/jlink-gdb-server.toml](examples/jlink-gdb-server.toml) 复制到本目录，并把 `service.command` 改为本机安装路径。

该 Server 没有 OpenOCD 的 TCL 端口，Memory access / Live refresh、寄存器专用读取和多核 CTI 同步都不可用；本地曾记录约 52–53 秒后长会话访问失效，使用前阅读用户手册的对应案例。

## 手动启动外部服务

~~~powershell
./tools/bin/openocd/bin/openocd.exe -s ./tools/bin/openocd/scripts -f ./tools/config/stm32f429-live.cfg
# 在另一个终端中运行：
debugtui --environment ./tools/debug-env.toml --connect 127.0.0.1:3333 --elf ./build/app.elf
~~~

`--connect` 禁止 TUI 启动配置中的 service，已有服务归调用者管理。OpenOCD 在 GDB 断开后继续运行，用 Ctrl+C 结束。

默认配置在退出前发送 `monitor resume`，再 detach，目标继续运行。若需要保持暂停，不要退出该环境会话；其他服务可使用不同的 `before_disconnect` 与 `session.on_exit` 策略。

`examples/openocd.toml`、`examples/riscv-external.toml` 是外部服务模板，需自行提供匹配的 GDB/服务。模板不代表完成对应实板验证。

## 独立打包

~~~powershell
./tools/package.ps1
~~~

生成 `artifacts/debugtui-tools-stm32-jlink-win-x64.zip`，解压后得到 tools 目录。打包前校验 `dependencies.lock.json`；`bin/` 下的文件变化后运行 `./tools/package.ps1 -UpdateLock` 重新生成该清单。TUI 版本升级无需重复安装工具集。

此最小工具集无 Python。GDB 的许可证位于 `bin/gdb/license.txt`；OpenOCD 的 GPLv2 文本和来源在 `bin/openocd/COPYING.txt`、`bin/openocd/PROVENANCE.txt`。应用源码许可证不替代第三方条款。
