# VFP 当前执行状态环境用例

2026-10-06。以下八项全部 **SKIPPED**；本任务不连接目标。正常启动固件建立权限、FP数据和独立每核RAM参考，DebugTUI只读驱动不切模式、不使能FP。

| Case | 准备及可执行入口 | 验收 | 状态 |
| --- | --- | --- | --- |
| FP-P01 | 当前Hyp／TCP10=0／EN=1，填写VFP case并运行下方只读驱动 | 所有控制／S/D/Q与独立RAM一致；每值带MIDR/EDSCR/DSPSR/DLR/HCPTR；保存状态与当前EL分别显示 | SKIPPED |
| FP-P02 | 先由正常固件在User或EL1停下，再由独立已知调试环境停留于当前Hyp；仅使用允许且可恢复的测试环境 | 当前Hyp依据外部EDSCR，与停止前DSPSR模式分开；不注入当前CPSR、CPU MIDR或模式切换 | SKIPPED |
| FP-P03 | 当前EL0/EL1，保存DSPSR可含旧Hyp状态；通过headless `registers_read` 手工请求VFP和r0 | VFP受限且实现不判为No；原生日志无CPU VFP/HCPTR/MIDR指令；Core及后续step有效 | SKIPPED |
| FP-P04 | 两核不同FP标本／停止状态，Scope All选中Core1；运行只读驱动的peer检查 | 数据／证明／route／context均绑定Core1；Core0、控制、客户配置原样 | SKIPPED |
| FP-P05 | Hyp正常固件分别设置TCP10=1与EN=0，运行只读驱动 | TCP10阻止VMRS；EN=0保留合法标识与FPEXC，拒绝FPSCR／数据；无隐式使能 | SKIPPED |
| FP-P06 | 在可控独立仿真或故障注入环境改变外部身份／EL／DLR、制造I/O或scratch故障 | target隔离，未知结果不发布，不继续注入、重试或猜测恢复；保存完整日志 | SKIPPED |
| FP-P07 | 已有有效值后接入旧v1或截短／矛盾响应的受控代理，重新刷新 | 新请求拒绝；旧值／原五项证明／请求时间保持；无GDB或旧adapter回退 | SKIPPED |
| FP-P08 | 独立可恢复实验环境，运行升级后的Python writer case；另注入首写前／两次Q之间的当前EL变化 | 安全未发送与可能部分完成准确区分；receipt证明不符停止，peer不变，不重试或自动回滚 | SKIPPED |

```powershell
node scripts/test-register-vfp-hardware.cjs
node scripts/test-register-vfp-hardware.cjs --run --binary PATH\debugtui.exe --project PATH\project.toml --core core0 --case PATH\vfp-case.json
python tools/openocd-adapter/tests/vfp-write-hardware.py --help
```

只读case从 `tests/fixtures/register-vfp-board.example.json` 复制；明确填写 `expected_debug_el=2` 与正常固件的 `expected_mode`，不得将两个字段互相推导。停止前DSPSR/DLR另与独立固件、GDB原始停止状态及PC核对，当前EL独立记录外部EDSCR。正常固件允许读取CPSR，与调试态禁止直接当前CPSR读取的规则分开。EL1/Guest合法VFP读取仍是REG-304缺口，不按受限结果宣称完成。

记录工具、profile、客户配置、固件ELF与case的SHA256、目标revision、每核私有ready／RAM可见性及前后控制。真实错误只在独立可恢复环境注入，软件JSON、模拟TCP或离线对象编译均不计为上板通过。
