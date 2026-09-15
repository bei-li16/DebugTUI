# 最小 STM32 GDB 调试工具

Windows x64，默认 STM32F429IG + J-Link + SWD。只保留 GDB、J-Link GDB Server、必需 DLL 和原厂许可证。无 Python、pip、串口/RTT 封装，也不需要安装 IDE 或配置工具 PATH。程序和资源全部随 Git 保存；克隆仓库后可直接使用其中的 tools。

## 两个终端即可调试

在仓库根目录打开两个 CMD 或 PowerShell 终端。

终端 1，启动服务并保持窗口打开：

```bat
.\tools\start_server.bat
```

终端 2，指定实际 ELF 文件：

```bat
.\tools\gdb.bat ".\STM32_CubeIDE\FreeRTOS_Project\Debug\FreeRTOS_Project.elf"
```

连接后目标暂停，直接输入原生 GDB 命令：

```gdb
p xTickCount
p/x g_w25q_jedec_id
info registers pc sp
break DEBUG_PRINTF
continue
step
next
stepi
watch Log_Tx_En
continue
bt
info breakpoints
delete breakpoints
```

`step/next` 按源码行执行，`stepi` 执行一条机器指令；`watch` 在变量变化时暂停目标。运行中的固件应选择会再次经过的位置设置断点，例如周期调用的 `DEBUG_PRINTF`。

### 复位运行、下载与退出

这是裸机远程调试，重新启动固件使用以下命令；不要把本机进程的 `run` 用法套在 MCU 上：

```gdb
delete breakpoints
monitor reset
monitor halt
maintenance flush register-cache
continue
```

需要下载当前 ELF 时，先暂停目标，再执行 `load`、`compare-sections -r`，然后复位运行。`load` 会写入固件；普通连接不会下载。

需要暂停正在运行的目标时按 Ctrl+C。结束调试并让目标继续运行：

```gdb
delete breakpoints
monitor go
detach
quit
```

服务使用 `-singlerun`：一个 GDB 连接结束后服务自动退出；下一次调试重新运行 `start_server.bat`。这也避免了本机旧 J-Link V8 长期复用 Server 后重连失败的问题。没有客户端时可在服务窗口按 Ctrl+C 结束。不要同时启动多个调试器占用探针。

### 批量命令和日志

ELF 后可直接追加原生 GDB 参数，命令在连接完成后执行：

```bat
.\tools\gdb.bat "firmware.elf" -batch -ex "p xTickCount" -ex "monitor go" -ex "detach"
.\tools\gdb.bat "firmware.elf" -batch -x "commands.gdb" > "session.log" 2>&1
```

交互会话需要记录输出时：

```gdb
set logging file session.log
set logging overwrite on
set logging enabled on
```

服务连接失败会在约 5 秒后返回非零退出码，并停止执行后续用户命令。`-ex` 多条命令沿用 GDB 原生退出码行为，后面的成功命令可能覆盖前面的错误；需要遇错停止时，把命令写入一个 `.gdb` 文件并通过 `-batch -x` 执行。

脚本不管理后台进程、不自动修改固件、不包装 GDB 的命令结果。批量脚本应自己包含结束时的恢复运行和断开命令；中途报错后需检查目标状态。

## 配置与迁移

`config/paths.bat` 只保存连接参数：

| 参数 | 默认值 |
|---|---|
| JLINK_DEVICE | STM32F429IG |
| JLINK_IF | SWD |
| JLINK_SPEED | 4000 kHz |
| GDB_PORT | 3333，仅本机连接 |

可修改此文件，或在两个终端中设置相同的环境变量。软件路径始终相对于当前 BAT 所在位置，外部 GDB_DIR/JLINK_DIR 等变量不参与程序查找。GDB 忽略系统和用户 gdbinit，数据、独立符号及自动加载目录指向当前 tools。保留 GDB 的目录结构和 `.keep` 文件，避免迁移后退回原厂编译目录。

ELF 和源码是工程输入，请自行提供，不属于 tools 的运行依赖。ELF 内的原编译路径失效时，在 GDB 中使用 `set substitute-path "原源码根目录" "新源码根目录"`，或在新位置重新编译。Windows 系统组件和 J-Link USB 驱动仍由目标电脑提供；tools 不包含编译器、构建工具或驱动安装器。

## 保留文件与来源

| 内容 | 用途 |
|---|---|
| start_server.bat / gdb.bat | 启动服务 / 连接原生 GDB |
| config/paths.bat / connect.gdb | 连接参数 / 连接检查与暂停初始化 |
| bin/gdb | STM32 工具链 GDB 14.2.90.20240526-git，约 9.92 MiB |
| bin/jlink | V7.94e Server + JLink_x64.dll + 许可证，约 22.07 MiB |
| dependencies.lock.json | 11 个第三方运行文件、许可证及目录占位文件的大小与 SHA-256 |

GDB 编译配置为 `--without-python`。J-Link 的约 21.54 MiB DLL 是此版本 Server 必需的原厂文件，未改写二进制。GDB 许可见 `bin/gdb/license.txt` 和 `ST-about.html`；J-Link 许可见 `bin/jlink/Doc/LicenseIncGUI.txt` 和 `ST-about.html`。再分发须遵守原厂许可。

已移除旧 dt 命令层、Python 运行环境、串口采集、RTT、J-Link Commander 和 OpenOCD。当前仅提供经过实板验证的 J-Link 服务链路；此前 OpenOCD 与本机 J-Link USB 驱动不兼容的问题并未通过本次精简修复。

Git 直接跟踪 EXE、DLL、许可证及目录占位文件，不使用 LFS、子模块或首次运行下载。只忽略生成日志、报告、固件和分发包。Git 历史仍包含旧版本文件，最新检出的 tools 已精简。
