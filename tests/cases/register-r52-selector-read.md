# R52 当前 Debug 证据的 MPU selector 延后测试

执行状态：**SKIPPED**。本 Goal 不执行上板测试。软件模型及 EXE 驱动验证不升级硬件 verified。

核心参考的绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

PRSELR/HPRSELR 范围依据 TRM §§4.3.87/.50，PDF 物理页 195–196/139–141；当前 Debug 状态另参 `G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf` F1.3.4/G2.1.8。PRSELR 表标题中的 EL2 文案矛盾沿用已有定义记录，不能因此混用 bank 容量。

使用专用固件及 ELF，提前停在 `debug_selector_fixture` 物理 frame 0，独占探针。记录 CPU revision、GDB/OpenOCD/DebugTUI 版本和摘要、每核 AP/target/GDB/Tcl endpoint；按实际硬件修改示例的独立期望值及容量，保留原 selector 和控制值。配置 `cp15_command="aarch64 r52_read"`、`selector_command="aarch64 r52_select"`、`isb_command=""`。AP 是已核实的连接配置，不是响应中的实测 AP 字段。

| Case | 操作和预期 | 状态 |
| --- | --- | --- |
| R52-SEL-H01 | 两个实际 R52 核分别读取 EL1/EL2 已实现的高 index，核对独立 BAR/LAR 与直接 index 值；selector、scratch、DSPSR/DLR、控制值及 peer 保持不变。保存程序可以是 User，授权须为当次 Debug EL2。 | SKIPPED |
| R52-SEL-H02 | 使用已有零 EL2/合法 16/20/24 固件配置；零容量不写 selector，越界参数不发数据命令，未知原 selector 不用于恢复；不得为此测试主动写非法 selector。 | SKIPPED |
| R52-SEL-H03 | 使用独立提供的当前低 EL/HDD/未适配身份环境验证执行前拒绝，不由驱动改模式、权限或控制位。故障测试使用专用可恢复环境，确认中止、FAULT、无盲目恢复/重试；按标准流程重连并重新 Probe。 | SKIPPED |
| R52-SEL-H04 | 请求提交后 Esc 取消、核/帧切换；事务正常完成时 selector 已恢复，新值被丢弃；旧值保留来源并按生命周期 stale。未知恢复必须隔离并重连，不能继续访问。 | SKIPPED |

准备和执行入口：

```powershell
# 默认准备检查：4 SKIPPED，零目标 I/O
node scripts/test-register-selectors-hardware.cjs
# 在专用环境中由用户明确执行；填写实际路径及独立 baseline
node scripts/test-register-selectors-hardware.cjs --run --binary <DebugTUI.EXE> --project <工程.toml> --core core0 --case <已核实的case.json>
```

驱动验证 H01 的值、当次证明、selector 恢复、控制和 peer；H02–H04 按表人工执行并保留日志，不把 H01 报告当作全部实板 case 通过。驱动不 halt/resume/reset/download，不使能模块，不修改 MPU、CPU 权限或模式。发生故障停止并保存日志，按芯片既定恢复流程重连；重新确认所有原始状态后再执行其他 case。

报告包含二进制/工程/case 摘要、实际请求与响应、成功/失败/跳过及 `board_tests_executed`。软件夹具执行加 `--software-fixture` 并保持 `board_tests_executed=false`；错误独立基线必须失败，不能改写基线适配读数。
