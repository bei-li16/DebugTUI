# Cortex-M MPU region 环境用例

全部 **SKIPPED**，本版未执行上板。默认驱动不连接目标。

环境准备：记录型号/revision、探针、OpenOCD/GDB 版本和摘要、专用固件/ELF、每核 AP/target/GDB/Tcl 端口、CorePrivate binding、测试输出目录及恢复流程。使用独立工程，结束其他调试客户端；由 GDB 停在专用测试函数的物理 frame 0。不使用 Tcl halt/run/step，不在测试中启用 MPU/FPU/DWT 或改配置。

从专用固件初始化表、对应手册及独立 Arm 调试器取得 CPUID、TYPE、CTRL、原 RNR、每个 region 的 RBAR/RASR 基线，分时独占探针采集。把 `tests/fixtures/m-profile-mpu-board.example.json` 复制到隔离位置，替换为该板的真实参数和独立期望，设 `software_example=false`。不同型号/容量分别准备 case，不复用软件常量当板级期望。

```powershell
# 没有 --run：三阶段全 SKIPPED，不连接目标
node scripts/test-m-profile-mpu-hardware.cjs

# 仅环境具备后显式执行；绝对路径与名称需填为该隔离工程
node scripts/test-m-profile-mpu-hardware.cjs --run --binary <EXE绝对路径> --project <隔离工程绝对路径> --core <物理核名> --case <独立基线JSON绝对路径>
```

## M-MPU-H01：三个型号和容量（SKIPPED）

M3、M4、M7 各运行一次驱动。具备不同实现时覆盖无 MPU、8 region、M7 的 16 region。Probe 核对本核 CPUID/TYPE，Read 校验全部 index 与独立原始对。无 MPU 时 CTRL/RNR/region 为未读取/空列表，Tcl 日志不得出现相关地址访问或 selector 写入。未知容量拒绝 bank 读取，不能默认按型号填数量。

预期：只访问有效 index；型号/容量/target/context/请求区间正确。读完 CTRL 和 RNR 与基线一致，MPU 配置及工程文件未变；仅事务中的 RNR 临时选择写入允许。恢复：核对 selector 已恢复，退出该测试 session，按固件的既定流程解除专用测试暂停状态。

## M-MPU-H02：M7＋M4、非连续核编号和缓存（SKIPPED）

配置 core0/core2 两个独立 target/AP 通道。分别准备两个 JSON 基线。在 Scope All 下逐核 Probe/Read，保存两个驱动报告和 TUI/Headless Snapshot；日志核对每批只有选中 target 的 RNR 写入/region 读取。切回第一核、滚动和字段查看只用其缓存；不带入另一核同地址的值、容量、错误和来源。独立核对两核 MPU_CTRL/RNR 及配置都未变化。

## M-MPU-H03：错误、取消、过期及恢复未知（SKIPPED）

先取得完整 bank。对可控的软件代理/测试环境分别制造选择回读不符、RBAR/RASR 读取拒绝、事务提交后的取消、完整读取结束前的线程/帧变化、响应断开及恢复回读不符。正常硅片测试不得靠写错误 MPU 配置制造故障；无法安全注入的阶段留给软件夹具，不宣称上板覆盖。

预期：已知错误尝试恢复原 RNR，不发布任何局部 region；原完整 bank 保留原采样/来源并 stale。取消等整个事务完成恢复后才丢弃结果。无法证明恢复或收到完整响应时进入 FAULT，后续请求零目标 I/O且无自动重试。只有手动核实目标/探针状态后才按既定连接流程恢复；不把重连本身当作 selector 已恢复的证明。记录每个注入点和恢复核验结果。

核心参考文档：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
