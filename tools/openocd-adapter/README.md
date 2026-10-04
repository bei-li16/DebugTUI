# OpenOCD ARMv8 寄存器适配补丁

这是独立的 GPL-2.0-or-later OpenOCD 修改，固定上游 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e` 和 Jim Tcl 子模块版本。校验和见 `source.lock.json`，补丁可以干净应用到该提交。现有 xPack Windows OpenOCD 没有被替换；相同的 `0.12.0` 版本号不证明具备本适配器。

`aarch64 mrrc cpnum op1 CRm` 对暂停的 AArch32 核执行一次 MRRC，将 R0/R1 的低、高字拼成 64 位结果，输出固定 16 位十六进制。它先保存物理 R0/R1，恢复后回读检查，不用缓存代替物理原值。补丁也使该命令组的 `aarch64 mrc/mcr` 保存、恢复并回读 R0。通用 `arm mrc/mcr` 和内部旧 DPM 路径没有被改造。

`aarch64 isb` 根据实际 DSCR 注入 AArch32/AArch64 的真正 ISB，不依赖或修改 CP15BEN。新通道先读 PRSR 确认物理核已暂停，检查错误与执行状态；传输、指令或恢复故障立即停止，把核心标为 unknown。不通过旧异常恢复函数切换模式，不自动重试或注入推测的回滚。成功路径没有 PC、CPSR 或 FPU 控制写入；调试异常时这些状态可能被硬件改变，应由操作者重连并决定复位恢复。

`aarch64 debugtui_adapter` 返回固定协议 `debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault`。在新编译的 DebugTUI 中显式配置：

```toml
[registers]
cpu = "cortex-r52"
cp15_command = "aarch64 mrc"
cp15_64_command = "aarch64 mrrc"
selector_command = "aarch64 mcr"
isb_command = "aarch64 isb"
tcl_endpoint = "127.0.0.1:6666"

[registers.targets]
core0 = "board.cpu0"
core1 = "board.cpu1"
```

核心和 target 名称须与实际工程一致。MRRC 与真正 ISB 每次在同一个 target 事务中检查协议；没有命令或协议不符时，先返回 reader unsupported。旧工程不配置新字段时，64 位读仍使用具名 GDB 寄存器，选择器同步仍要求实际 CP15BEN 已开启。配置不能代替硬件身份、Timer 实现或权限证据，也不开启系统寄存器 writer。

## 构建和软件验证

在具备 Git、GCC、make、autoconf、automake、libtool 和 pkg-config 的 Unix 构建环境中执行：

```sh
sh tools/openocd-adapter/build.sh .dev/openocd-adapter-new-build
```

输出目录必须是新路径。脚本保留固定版本源码、安装目录、GPL 许可和测试报告；可追加 configure 选项选择探针及交叉工具链。USB 接口需要对应开发依赖。默认包含 dummy/remote-bitbang，关闭未准备的 J-Link 子模块，不能覆盖所有生产探针。本批在 WSL Ubuntu 22.04 完成 Linux 构建与真实命令检查；Windows 生产后端、探针依赖和发布包仍待完成。一键脚本通过语法、补丁应用检查，全新目录的完整构建尚未重新执行。

检查已应用补丁的源码和后端：

```sh
python3 tools/openocd-adapter/test.py --source PATH_TO_PINNED_SOURCE --out artifacts/openocd-adapter-tests --openocd PATH_TO_BACKEND
```

事务测试编译生产使用的同一份头文件，验证完整高字、一次 MRRC、MRC/MCR 的物理恢复、19 个传输失败点和三类恢复值不匹配，严格 C 警告检查通过。命令检查仅初始化进程内 dummy 虚拟适配器，保持 target 未 examine，核对协议、帮助以及参数和状态的精确原生错误码；所有端口关闭，不连接实际探针/板卡。`tests/encoding.s` 用 GNU Arm 汇编器独立确认 MRRC 和 Thumb ISB 编码。

Linux 候选后端 SHA-256 为 `80f2c574531855a50c6e4f52e9760d1603a296c3d8cca1579cbd995bfe245bd5`，版本 `0.12.0+dev-gd3ebb8d-dirty`。dirty 来自尚未成为上游提交的适配补丁。候选产物未安装或发布，未来分发须保留源码、补丁与许可，不能把其能力写到现有 Windows 二进制上。

## 延后执行的物理核心用例

`scripts/test-register-timer-hardware.cjs` 默认输出五项 skipped，不连接。专用暂停工程须禁止连接/退出时自动运行、复位或下载，并设置 `on_exit = "disconnect"`。明确提供 `--run --binary EXE --project TOML --core NAME --case JSON` 才执行 REG-H05。`tests/fixtures/register-timer-board.example.json` 是软件占位，需替换实际 MIDR、CVAL、合理差值和独立证据，移除 `software_example` 后才能上板执行。

只读钩子 `tests/fixtures/register-timer-board.c` 记录 CNTPCT/CNTVCT/CNTP_CVAL 的固件 MRRC 样本，不开启 Timer 或修改比较值。各核分别编译固件或将基线改为 per-core 数组，据此修改 case 的变量引用。固件初始化需建立权限、明确 debug freeze 条件，并配置至少一个高字非零的已知稳定 CVAL。驱动按完整 64 位模运算检查基线差值和进展，避免要求不同时间采样相等；检查当前核、稳定控制、选择器、可选 peer 和客户配置未变。GDB 的核心 guard 是逻辑视图，不能单独证明物理 R0/R1 恢复；物理回读由固定适配器内部执行。

真正 ISB 的物理用例复用 `scripts/test-register-selectors-hardware.cjs`：专用固件预先关闭 CP15BEN，工程设置 `isb_command`，分别执行 EL1/EL2 MPU 和 PMU 已知索引，保留 `evidence.synchronization`、原/恢复选择器及控制状态对比。不得为通过用例而启用 CP15BEN；未知结果不自动继续、复位或重试。Timer 驱动已在实际 DebugTUI 可执行文件的双核软件模型六阶段通过，包含独立基线接口和 peer 检查。当前均未上板执行，Linux 编译和软件模拟不记作板卡支持证明。
