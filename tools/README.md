# 可选调试环境

此目录独立维护 GDB、探针服务、配置与许可证。DebugTUI 0.2 的核心和安装包不依赖此目录结构，也不会自动寻找它。

## STM32/J-Link 环境

debug-env.toml 包含所有环境差异：ARM GDB 路径和资源目录，J-Link 服务参数，STM32F429IG / SWD / 4000 kHz / 3333，以及连接暂停、复位、烧录和退出恢复命令。

~~~powershell
debugtui --tools-dir ./tools --elf ./build/app.elf
~~~

复制 debug-env.toml 可以建立不同板卡的配置。修改服务端口时同步修改 service.args 和 target.endpoint；修改芯片时修改 service.args 中的型号。TUI 无需重新编译。

GDB/Server 路径按此 TOML 所在目录解析。${profile_dir} 用于环境自己的资源路径。原有 BAT 入口继续使用 config/paths.bat；TUI 使用 TOML，不读取 BAT 环境变量。

## 外部服务

~~~powershell
./tools/start_server.bat
# 在另一个终端中运行：
debugtui --environment ./tools/debug-env.toml --connect localhost:3333 --elf ./build/app.elf
~~~

--connect 禁止 TUI 启动配置中的 service。已有服务归调用者管理。J-Link -singlerun 自身在断开后退出，这不是 TUI 杀死外部进程。

默认配置在退出前发送 monitor go，再 detach；J-Link 服务正常退出也会恢复 MCU。若需要保持暂停，不要退出该环境会话。其他服务可使用不同的 before_disconnect 与 session.on_exit 策略。

examples/openocd.toml、examples/riscv-external.toml 是外部服务模板，需自行提供匹配的 GDB/服务。模板不代表完成对应实板验证。

## 独立打包

~~~powershell
./tools/package.ps1
~~~

生成 artifacts/debugtui-tools-stm32-jlink-win-x64.zip，解压后得到 tools 目录。打包前校验 dependencies.lock.json；TUI 版本升级无需重复安装工具集。

J-Link USB 驱动和 Windows 系统组件由宿主提供。此最小工具集无 Python。GDB 的许可证位于 bin/gdb/license.txt，J-Link 条款位于 bin/jlink/Doc/LicenseIncGUI.txt；应用源码许可证不替代原厂条款。
