# R52 当前 Debug 证据下的 MPU 总览用例

本批不执行上板，四项用例均为 **SKIPPED**。核心参考绝对路径完整保留：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

当前 Debug 规则使用补充 `G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf` 的 F1.3.4/G2.1.8，编号 ARM DDI 0568A.c。指定架构介绍不是完整指令/External Debug 正文。

准备专用固件、ELF 和隔离工程。独占探针，记录实际 R52 revision、探针、DebugTUI/OpenOCD/GDB 版本和 SHA256、每核 AP/target/GDB/Tcl endpoint、配置与恢复方式。配置 `cp15_command="aarch64 r52_read"`，本批的旧 selector/ISB 字段清空。由既有 GDB 控制提前停在专用函数、物理 frame 0；驱动不 halt/resume/reset/download，不修改 MPU/cache/权限/模式。

从固件初始化表和独立调试器取得每核全部 BAR/LAR、MAIR、控制、原 selector、MIDR、容量及保存的 DSPSR。将 `tests/fixtures/r52-native-mpu-board.example.json` 复制到隔离位置，替换真实期望、工具版本、AP/target/endpoint、函数和两个核心名称，设置 software_example=false。current_debug 内的路由/AP/工具是用户声明，报告不能把这些声明当作探针扫描或工具版本实测。原始值、实际响应和日志分别保留。

```text
node scripts/test-mpu-regions-hardware.cjs
node scripts/test-mpu-regions-hardware.cjs --run --binary <EXE绝对路径> --project <隔离工程绝对路径> --core <物理核名> --case <独立基线JSON绝对路径>
```

第一条默认四项 SKIPPED，零目标 I/O。第二条显式执行，报告正向四阶段及 cleanup；任一前置失败，后续阶段 SKIPPED。软件入口测试另使用 --software-fixture，结果 board_tests_executed=false，不能证明 ARM 指令或实板能力。

## R52-MPU-H01：当前 EL2 与保存 User 状态（SKIPPED）

专用固件在 User 程序中停住，独立确认外部 Debug 状态满足当前 AArch32 EL2/HDD=0。运行上述驱动，核对两 bank 的全部有效 index、MAIR 解码与独立原始值；GDB CPSR 和后端 DSPSR 作为停止前程序状态，权限来源为每次后端 MIDR/EDSCR。每个数据样本须为当前 core owner/context/target/endpoint，并保留完整请求区间与 r52_core 证明。

预期：User 保存状态不阻止当前已证明 EL2 的合法访问；无 selector/control MCR、模式切换、自动使能或 GDB 数据替代。批内身份、完整 DSPSR/DLR、状态与容量一致，最后再读身份/容量验证。纯缓存查看、滚动与字段说明不产生额外 I/O。恢复：驱动关闭自己的会话，按固件既定流程离开专用停止点。

## R52-MPU-H02：双核、独立容量及旧上下文（SKIPPED）

Scope All 下先运行 core0 的独立 case，驱动验证 peer 控制/selector 未变；再用 core1 独立 region 基线运行一次，交换 peer 配置。实际有不同容量的 build 时覆盖 16/20/24，记录每核的原始 MPUIR/HMPUIR，未具备的组合分别 SKIPPED。TUI/Headless 切核后重新 Probe，再读本核总览；提交旧 context 请求应在数据访问前拒绝。

预期：每批只访问选中 target，同 index 的值、容量、错误与来源不串核；另一核控制/selector 不变。恢复：保留两次独立报告与日志，关闭隔离会话。

## R52-MPU-H03：零 EL2 与读取边界（SKIPPED）

仅在实际 EL2 MPU 未实现的 build 上，把 el2 的 count/regions 分别填 0/[]。运行驱动并单独检查 registers_mpu 日志：只有 MIDR/HMPUIR 和最终验证，零 region/MAIR/MPU control 读取，空列表没有伪造 0 值。驱动对稳定控制的独立基线读取应单独标明，不冒充总览的可选读取。

使用已确认是非 R52/R52+ 未适配身份或当前低 EL 的专用环境时，记录执行前的明确 Unsupported/Unknown/Restricted；不得通过写权限或切模式制造允许。恢复：退出隔离会话；缺少相应 build 的情况继续 SKIPPED。

## R52-MPU-H04：取消、变化和恢复未知（SKIPPED）

先取得完整成功总览。在 MPU 总览弹窗下一批读取期间按 Esc 关闭并取消，或在受控软件代理中制造线程/帧、两事务之间 DSPSR/DLR/身份/容量变化、读取拒绝、响应断开或后端恢复不确定。正常硅片不写非法 MPU/权限制造故障。没有安全注入设施的情况保留软件证据，实板部分继续 SKIPPED。

预期：取消等当前事务完成恢复后丢弃整批；拒绝/变化立即停止，已读部分不发布。旧值、时间与原证明保留并按状态失效。恢复不确定为 FAULT，后续访问被隔离，不自动换 MRC/GDB 或重试。操作者根据故障日志决定重连/复位；测试驱动不猜测恢复写入。
