# WRITE-H03 / H04 / H05：VFP 原始写入

以下硬件用例均为 **SKIPPED / 未执行**。已接入 OpenOCD 底层事务及 DebugTUI 的独立 S/D/Q writer、preview/apply/cancel、物理核服务锁、键鼠编辑和写后旧视图失效。软件测试不证明实板能力；合法 EL1/Guest/User、FP 状态 writer 和完整跨面板／工具集验收仍未完成。

依据：本地 Cortex-R52 TRM `100026_0104_01_en` §§16.3–16.6；完整 Armv8-R AArch32 Supplement `DDI 0568A.c` D1.3 的 VMOV 两个 GP 与 D 存储的合法指令子集。D16 SP-only 的 D 是 64 位存储，可搬运原始位，但不说明支持双精度运算。S0–31 映射 D0–15；Q0–15 是 D0–31 两两组合，只有已确认的 D32/NEON 配置开放 Q。

准备专用固件，让启动代码明确建立 Hyp、HCPTR.TCP10=0、FPEXC.EN=1 和已知 D 数据；两个核心分别在独立 ready loop 暂停，测试期间独占调试器，排除 Live Watch、GDB 或其他 TCL 客户端并发。不要用本测试打开 FPU、切换模式、复位、下载或暂停生产程序。记录探针、工具哈希、MIDR、MVFR0/1、FPEXC、FPSCR、HCPTR、DSPSR、物理 R0/R1/PC 与每核原始 D 基线。使用 [独立 GNU VMRS/VMOV 固件钩子](../fixtures/register-vfp-board.S) 采样初始原始字，不以 writer 返回的 expected 作为独立基线。

默认执行不会连接：

```powershell
python tools/openocd-adapter/tests/vfp-write-hardware.py
```

环境具备后复制 [case 模板](../fixtures/register-vfp-write-board.example.json)，替换 endpoint、target、peer_target、register、raw 和两核已知物理 pair，移除 `software_example`。`expected_before` / `expected_peer` 始终为 128 位（高 D 在左、低 D 在右），即使修改 S/D。`fixture_core_private` 与 `restore_on_verified` 必须为 true，表示操作者已确认测试存储及成功后的显式原值写入。驱动不启动 OpenOCD、不执行目标代码，使用已连接的独立后端：

```powershell
python tools/openocd-adapter/tests/vfp-write-hardware.py --run --case G:/Tests/r52-core0-s31.json
```

驱动核对固定协议和暂停 owner、独立基线、原始写入及完整 pair、FP/模式/陷阱控制和 peer；只在所有验证通过后再发送一次独立恢复写入。任何 unknown、mismatch、断连、超时、控制变化或恢复故障停止后续指令，不重试、不自动 rollback。内部事务物理回读 R0/R1；驱动的外部控制复核不能代替独立物理 PC/GPR 或恢复运行后固件样本。这些仍需保存为各用例的额外证据。

DebugTUI 主机链路另有独立驱动，默认生成 4 skipped，不启动或连接目标：

```powershell
node scripts/test-register-vfp-write-hardware.cjs
```

复制 [主机 case 模板](../fixtures/register-vfp-host-write-board.example.json)，填写真实工程、暂停钩子、两核 target、MIDR/MVFR 和独立 pair，移除 `software_example`。固件参考数组布局沿用 `register-vfp-board.S`，pair 四个 32 位原始字在 `[8 + pair*4 .. 11 + pair*4]`，ready 符号为 `${reference}_ready`；peer 可使用自己的 `peer_reference`，不填写时在对应物理核读取同名独立数组。工程启用独立读写协议、各核明确 target，使用 `on_exit="disconnect"`，禁用连接时运行／复位动作。

```powershell
node scripts/test-register-vfp-write-hardware.cjs --run --project G:/Tests/r52-writer.toml --core core0 --case G:/Tests/r52-host-s31.json --binary G:/Tests/debugtui.exe
```

该驱动使用实际 `--headless --stdio` 写接口，核对独立固件初始字与暂停 FP pair、取消后的原样数据、一次 Apply 及重复令牌拒绝、全部有效 S/D/Q 别名、控制和 peer，并在全部 verified／独立读取通过后显式恢复。它不运行、暂停、下载、复位或使能 FPU。unknown 会隔离共享服务，退出也可能报告目标状态需检查；保存原始错误并停止，不为使报告通过而重连／重放。软件夹具以 `--software-fixture` 标明，不能作为上板证据。

| Case | 操作 | 验收条件 |
|---|---|---|
| VFP-WH-S | core0/core1 分别选 s0/s1/s30/s31，写正负零、Infinity、qNaN/sNaN payload 原始位；分别执行模板驱动 | 改变所选 32 位，D 的另一半及 pair 的另一 D 原样保留；FPSCR 不因位搬运改变；peer 不变；不发生浮点数值转换 |
| VFP-WH-D16 | 在已确认 SP-only D16 的专用固件，对 d0/d15 和全部 S 边界运行驱动；手工请求 d16/q0 | D 原始 64 位可写；高 D/Q 返回 not_sent/not-implemented，未注入数据写入；不能将 D 位搬运记为双精度运算支持 |
| VFP-WH-D32-Q | D32/NEON 配置写 d16/d31、q0/q15，含高 64 位非零与独立不对称字 | D 与 Q 高字完整，S/D/Q 位别名一致；Q 明确 non-atomic、两个 D 各写一次，不使用 GDB LONGEST 通道 |
| VFP-WH-PERMISSION | 分别准备 EN=0、TCP10=1、EL1/Guest/User、未知 MIDR/MVFR 的独立固件；手工运行协议命令 | 当前后端按真实前置条件返回 not_sent；没有写 FPEXC/CPACR/HCPTR、模式或尝试越权 VMOV；EL1/Guest 当前未适配不算硬件缺失 |
| VFP-WH-CACHE | 在独立可恢复会话中预设同一 pair 的 GDB D 写入；另测待写 PC/CPSR/FP 状态，再请求后端写入；成功后刷新 D 别名 | pending-register-write 在注入前拒绝；保留原 GDB 待写值。无待写值时，成功或未知结果使该 pair 与两个 ARM32 D alias 的后端有效标记失效；恢复运行不回放旧 pair |
| VFP-WH-STATE | 成功写入前后保存独立物理 R0/R1、PC、完整 DSPSR（含 T/IT）、HCPTR、FPEXC、FPSCR | 只有指定 FP 数据变化；所有控制、临时寄存器与 peer 原样；写后显式恢复原数据再次独立确认 |
| VFP-WH-PARTIAL | 仅在独立可恢复实验环境注入第一个/第二个 D 写入后的传输失败、scratch 恢复或控制回读故障 | 后端 target unknown，日志提示重连；不注入剩余指令、不重放、不猜测恢复。Q 可能已部分改变，不能宣称 not_sent/atomic。当前 144 故障点仅有生产 C 事务软件证据 |
| VFP-WH-FRONTEND | 主机驱动分别运行 core0/core1 的 S/D/Q 正常 case；另在 TUI 45×12 和 80×24 终端操作键盘 `e`、鼠标 Edit value、Preview、Cancel、Apply；预览后切换核／帧／停止点／会话／权限，测试 Scope All；两个别名草稿先应用一份再尝试另一份 | 默认无写；取消或旧上下文不写；写入只属于选中 owner，peer 原样；S/D 新鲜邻接位与 Q 高字完整；其他草稿及旧别名失效，手动刷新由新 pair 得到一致值。后端协议不符无 GDB 回退；unknown/timeout 不重试。已有实际二进制＋真实 Tcl 软件证据及键鼠单元测试，实板／原生终端视觉验收仍未执行 |

成功后允许操作者按固件流程恢复运行，使用独立 VMOV 钩子确认结果及恢复值，再进行下一例；把新的暂停点当作新上下文。自动驱动没有这一步，不把软件 TCP 夹具或后端输出算作独立固件执行证明。
