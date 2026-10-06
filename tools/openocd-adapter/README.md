# OpenOCD ARMv8 寄存器适配补丁

这是独立的 GPL-2.0-or-later OpenOCD 修改，固定上游 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e` 和 Jim Tcl 子模块版本。校验和见 `source.lock.json`，补丁可以干净应用到该提交。现有 xPack Windows OpenOCD 没有被替换；相同的 `0.12.0` 版本号不证明具备本适配器。

当前补丁另提供有界普通 R52 读取 `aarch64 r52_read NAME` 及独立 `debugtui_r52_protocol`，见 [配置、当前 EL2 与事务边界](../../docs/register-r52-core-read.md)。本批实际生产 C 模型与 host 集成已验收，MPU 总览/selector 统一接入及完整新 OpenOCD 命令入口仍待完成；`source.lock.json` 的 Windows/Linux build verified 为 false、候选摘要为空。以下已构建候选的描述是各旧子批次的历史证据，不能证明当前补丁构建、当前安装或实板执行。新命令不接受任意 CP15 编码，不与旧 selector 参数直接混用。

`aarch64 mrrc cpnum op1 CRm` 对暂停的 AArch32 核执行一次 MRRC，将 R0/R1 的低、高字拼成 64 位结果，输出固定 16 位十六进制。它先保存物理 R0/R1，恢复后回读检查，不用缓存代替物理原值。补丁也使该命令组的 `aarch64 mrc/mcr` 保存、恢复并回读 R0。通用 `arm mrc/mcr` 和内部旧 DPM 路径没有被改造。

`aarch64 isb` 根据实际 DSCR 注入 AArch32/AArch64 的真正 ISB，不依赖或修改 CP15BEN。新通道先读 PRSR 确认物理核已暂停，检查错误与执行状态；传输、指令或恢复故障立即停止，把核心标为 unknown。不通过旧异常恢复函数切换模式，不自动重试或注入推测的回滚。成功路径没有 PC、CPSR 或 FPU 控制写入；调试异常时这些状态可能被硬件改变，应由操作者重连并决定复位恢复。

`aarch64 debugtui_adapter` 返回固定协议 `debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault`。在新编译的 DebugTUI 中显式配置：

```toml
[registers]
cpu = "cortex-r52"
cp15_command = "aarch64 mrc"
cp15_64_command = "aarch64 mrrc"
timer_command = "aarch64 timer"
pmu_command = "aarch64 pmu"
gic_command = "aarch64 gic"
selector_command = "aarch64 mcr"
isb_command = "aarch64 isb"
banked_command = "aarch64 banked"
vfp_command = "aarch64 vfp"
tcl_endpoint = "127.0.0.1:6666"

[registers.targets]
core0 = "board.cpu0"
core1 = "board.cpu1"
```

核心和 target 名称须与实际工程一致。MRRC 与真正 ISB 每次在同一个 target 事务中检查协议；没有命令或协议不符时，先返回 reader unsupported。旧工程不配置新字段时，64 位读仍使用具名 GDB 寄存器，选择器同步仍要求实际 CP15BEN 已开启。配置不能代替硬件身份、Timer 实现或权限证据，也不开启系统寄存器 writer。

`aarch64 debugtui_banked_protocol` 返回独立协议 `debugtui-armv8-banked-2 external-identity current-el dspsr dlr mrs physical-readback no-mode-change stop-on-fault`。旧 v1 不满足新鲜当前 Debug 状态证明，DebugTUI 在数据指令前拒绝旧协议，不回退 `get_reg`。`aarch64 banked NAME` 仍接受三十个小写名称，内置目录仍为二十三项；其余 User 银行可以通过自定义目录显式接入。

身份取自外部 Debug MIDR（0xD00），当前 EL 取自 EDSCR（0x88），每次外部访问都验证 EDPRSR.HALT。仅接受 Arm/D13/架构F/AArch32。EL2 唯一为 Hyp，允许全部三十项；当前 SP/SPSR 使用 MOV32/MRS32，ELR_hyp 及其他银行用合法 banked MRS。EL0 唯一为 User，只允许七项当前 User 银行的 MOV32，其他银行明确受限。EL1 的具体 FIQ/IRQ/SVC/ABT/UND/System 模式无法由 EDSCR 证明；三个 Hyp 银行受限，其余返回 `debugtui-banked:access-unknown`，在 CPU 指令前停止。完整 EL1 支持保持待完成。

停止前 DSPSR 与当前模式分开：DSPSR/DLR 前后全位复核，不用保存的模式选择当前银行。DDI0487 M.b H2.4.2.2/H2.4.8.2 将调试态直接 CPSR/PSTATE 读取定义为受约束不可预测，因此不注入 MRS CPSR；H2.4.2.2.2 明确允许普通 MRS SPSR。银行访问规则使用 DDI0568A.c H1 BankedRegisterAccessValid/SPSRaccessValid。每次传输保存、恢复并物理回读 R0，前后核对外部身份、当前执行状态及 DSPSR/DLR；无模式/控制写入、重试或推测 rollback。恢复/身份变化/传输不确定立即隔离 target，不发布部分结果。

成功响应固定为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 value RAW32 method mov32|mrs32|banked_mrs32`。主机严格核对字段、实际名称、身份/EL、合法方法和位宽；证据绑定当前 owner/context/route/请求时间，详情及 headless JSON 可查，失败旧值保留原有证据。旧 JSON 缺少 banked 字段保持缺失，不制造证明。不同银行和不同核分别采样。

生产 C 模型覆盖37项成功、41项受限、162项 EL1 Unknown 和1110个 I/O失败点，另核对三十条 GNU 指令字与身份/状态/DSPSR/DLR/scratch 变化。Windows/Linux 在新目录重建，七协议及本机 DLL/离线配置校验通过；候选散列见 source.lock。详见 [银行当前 Debug 状态自检](../../docs/register-banked-proof.md)。未执行上板，未替换全局安装，未发布 Release。

## Timer 当前 Debug state 读取

`aarch64 debugtui_timer_protocol` 返回 `debugtui-armv8-timer-1 external-identity current-el dspsr dlr scratch-readback no-mode-change stop-on-fault`，`aarch64 timer NAME` 接受 R52 Table 11-1 的十五个小写名称。DebugTUI 显式配置 `registers.timer_command="aarch64 timer"` 后按 CP15 编码选择专用路径；错误协议、拒绝或截断响应均不回退旧 MRC/MRRC 或 GDB。空配置保留原通道。

当前 EDSCR.EL/RW/HDD 和 debug AP 的 MIDR（偏移 0xD00）提供新鲜状态与身份，避免在低 EL 注入可能被 HSTR 捕获的身份指令。仅接受实际 Arm D13/AArch32，R52+ 未知身份保持 unsupported。依据为 R52 TRM Table 12-5、DDI0568A.c F1.3.4/H1-255、工作区完整 DDI0487 M.b H2.4.5/H2.4.8。Armv8 系统寄存器保持当前 EL 的权限与 trap；旧 Armv7 的 CP15 调试权限描述不能覆盖此规则，HDD=0 不等于绕过 trap。

EL2 允许全部十五项。EL1 在 HDD=0/1 时允许 CNTFRQ、CNTKCTL、CNTV_TVAL、CNTV_CTL、CNTVCT、CNTV_CVAL；Timer CRn/CRm=14 不受 HSTR coarse trap。EL1 的四项物理 Timer 依赖不可从 EL1 读出的 CNTHCTL，返回 `access-unknown`，不注入 Timer 指令；五项 Hyp-only 返回 `access-restricted`。EL0 的 CNTKCTL/Hyp-only 明确受限，其余项目依赖不可在 EL0 读出的上层使能，保留权限未知。不用配置、缓存 CPSR、旧 Probe 或 HDD=0 猜测允许，不改变 EL/模式取得控制位；完整低 EL 受控访问仍待适配。

成功路径核对前后外部身份和 EDSCR、全部 DSPSR 与 DLR，保存/恢复/物理回读 R0/R1；每次外部 AP 读取还检查 EDPRSR.HALT。六项 64 位各执行一次 MRRC；不同条目分别采样。响应为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 value RAW32|RAW64`。原始证据绑定读取来源，在 headless JSON 和详情窗口中可查，并随旧值保存。故障或状态变化不发布部分结果，立即标记 target unknown；不执行模式切换、异常恢复、重试、Timer 控制写入或推测回滚。

`tests/timer-transfer.c` 编译同一生产头文件，独立手写十五项指令字，验证 474 个失败点、12 个合法 EL1/HDD 组合、48 个权限拒绝及身份/完整状态/PC/暂存值变化。最新 Windows/Linux 候选哈希以 `source.lock.json` 为准，此前 VFP writer 候选记录在 previous_candidates。软件模型与离线命令检查没有执行目标指令，新候选未安装或发布。

Timer 一致性自检另覆盖 48 个动态回读模型（六项×八个边界值）。在 MRRC 已复制 pair 后，模型计数器在物理 R0/R1 回读之间前进，低字进位/全值回绕不能撕裂输出。两字都来自同一次指令，后端不追加计数器试读或重试。Windows/Linux 同一生产头文件测试通过，固定源码/补丁和候选 EXE 均未改变；当前测试与源码重新封装、原生 Windows 验证和 Linux dummy 命令证明在 `artifacts/openocd-timer-coherence-windows-native.log`、`artifacts/openocd-timer-coherence-linux/report.json`。对应源码 ZIP 保留新测试，运行文件哈希仍以 source.lock 为准。DebugTUI 另保存请求起止主机区间与单项 MRC32/MRRC64 方法，不能据此声称跨项同时性或真实硬件采样时间。

## VFP 原始值与别名

`aarch64 debugtui_vfp_protocol` 返回 `debugtui-armv8-vfp-2 external-identity current-el dspsr dlr vmrs pair-readback no-enable stop-on-fault`。`aarch64 vfp NAME` 接受 FPSID、FPSCR、MVFR0/1/2、FPEXC（小写名称）以及 D0–31、Q0–15。控制值为 32 位，数据始终返回两个 D 寄存器组成的 128 位物理采样，并附 MVFR0、MVFR1 和 FPEXC 原始证据。DebugTUI 按请求缓存物理 pair，D 的两个 lane、S 的低／高 32 位和 Q 共用同一值；不会把截断的 GDB 输出补成宽值，也不回退旧 get_reg。

R52 TRM §§16.5–16.6 的两种配置为 SP-only D16（MVFR0=`0x10110021`、MVFR1=`0x11000011`）和 DP/NEON D32（`0x10110222`、`0x12111111`）。DDI 0568 D1.3 明确 SP-only 仍允许 GP 两寄存器与 D 的 VMOV；D16 中超过 D15 或 Q 视图明确未实现。未知／矛盾字段保留原始控制值，拒绝数据访问；FPINST/FPINST2 不实现，不试探。

当前专用通道使用外部 Debug AP MIDR 和 EDSCR 证明实际 R52 D13／当前 Hyp；每次外部访问核对 EDPRSR.HALT。保存的 DSPSR 仅是停止前状态，不授权当前指令，不注入 CPU MIDR 或 MRS CPSR。当前 EL0/EL1 在 CPU 指令前拒绝；当前 Hyp 再读取 HCPTR.TCP10，受限时不执行 VMRS。Hyp 不依据 CPACR 推断权限。FPEXC.EN=0 仍可读取标识／FPEXC，FPSCR 和数据返回 Feature disabled。EL1/Guest/User 当前安全返回 Access restricted，其合法 VFP 读取仍是待补后端／权限策略；这不是硬件未实现证明。读取不会切换模式、写 CPACR/HCPTR/FPEXC/FPSCR 或 FP 数据。

每次保存／恢复／物理回读 R0/R1，复核前后完整 DSPSR、DLR、HCPTR、FPEXC，以及外部身份和当前执行状态；传输／指令／恢复结果未知立即停用目标，不继续注入或重试。成功响应固定为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 hcptr RAW32 mvfr0 RAW32 mvfr1 RAW32 fpexc RAW32 value RAW32|RAW128`。主机拒绝旧协议、缺失或矛盾的当前状态；所有控制及数据值保留同次状态证明，旧 JSON 不伪造新字段。S/D/Q 的共享缓存仅在同一已校验的读取请求内有效，不跨核／停止点／线程／帧复用。Scope All 只访问选中核心。

## 构建和软件验证

### VFP 原始位写入后端

`aarch64 debugtui_vfp_write_protocol` 返回独立协议 `debugtui-armv8-vfp-write-2 external-identity current-el dspsr dlr vmov raw-pair fresh-merge scratch-readback no-enable stop-on-fault`。`aarch64 vfp_write NAME RAW` 只接受 S0–31、D0–31、Q0–15 的小写名称和精确 32/64/128 位十六进制原始值，不执行表达式或浮点数值转换。FPSCR、FPEXC 和标识控制不在 writer 允许名单中。当前只支持实际 R52 D13 的 Hyp／TCP10=0／EN=1；其他模式的合法路径仍待适配，不自动改变 CPU 模式或使能 FPU。

事务复用已校验的物理 pair 读取，在发送前重新确认实际权限／能力。S 从新鲜 D 保留另一半 32 位，D 保留 pair 的另一 D；Q 用两次 VMOV 写入，明确非原子。每次都保存／恢复／物理回读 R0/R1，写后再读取完整 pair，每个物理写入前再次核对 EDSCR，写后比较前后 MIDR、EDSCR、DSPSR、DLR、HCPTR、FPEXC 和 MVFR；NaN payload、符号和高字直接按位搬运。结果为 `outcome verified|mismatch before RAW128 expected RAW128 value RAW128 mvfr0 RAW32 mvfr1 RAW32 fpexc RAW32 midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 hcptr RAW32`。前置条件安全拒绝为 `outcome not_sent reason REASON`。任何传输、恢复或状态结果未知都立即停止并把 target 标为 unknown，不重试、补写或猜测回滚。

同一 pair 或 PC/CPSR/FP 状态的 OpenOCD GDB cache 有待写值时，注入前拒绝为 pending-register-write；不删除用户的待写值。实际发送后使该 pair 及两个 ARM32 D alias 的有效标记失效。只访问当前明确 target，无跨核广播。DebugTUI 已以独立 `vfp_write_command` 接入 Hyp raw preview/apply、编辑 UI、服务锁和别名失效；EL1/Guest/User、FP 状态及其他 writer 类别仍待适配，这个后端不能代替 WRITE-005/008–012 完整验收。

VFP 读取生产模型验证73个故障点、八种保存模式与当前Hyp分离、EL0/EL1零CPU指令拒绝、外部身份／执行状态／DLR变化。`tests/vfp-write-transfer.c` 编译生产头文件，验证 80 个视图、166 个故障点、两次 Q 写入的部分完成、临时寄存器恢复、完整 pair 及控制一致性、D16/未知身份/未使能/陷阱拒绝；GNU Arm 独立核对 VMOV 写入编码。`tests/vfp-write-driver.py` 用真实 TCP 的严格双核模型验证三种位宽、明确恢复和失败后不读／重试／回滚，8 项通过；另验证DTR传输标志变化不误判当前EL，以及当前EL1在写入前拒绝。硬件驱动 `tests/vfp-write-hardware.py` 默认 4 skipped；模板和八类延后用例见 [VFP 写入 case](../../tests/cases/register-vfp-writes.md)。上板执行未做，外部物理 PC/GPR、待写 cache 及独立固件写后样本仍需额外记录。

在具备 Git、GCC、make、autoconf、automake、libtool 和 pkg-config 的 Unix 构建环境中执行：

```sh
sh tools/openocd-adapter/build.sh .dev/openocd-adapter-new-build
```

输出目录必须是新路径。脚本保留固定版本源码、安装目录、GPL 许可和测试报告；可追加 configure 选项选择探针及交叉工具链。USB 接口需要对应开发依赖。默认包含 dummy/remote-bitbang，关闭 J-Link 子模块。本 VFP 写入后端批次已用上述 Linux 一键脚本在全新目录完成 WSL Ubuntu 22.04 构建，最终修复增量重编译后重新完成真实命令检查。Windows 使用下面单独经过全新目录构建验证的脚本。

检查已应用补丁的源码和后端：

```sh
python3 tools/openocd-adapter/test.py --source PATH_TO_PINNED_SOURCE --out artifacts/openocd-adapter-tests --openocd PATH_TO_BACKEND
```

事务测试编译生产使用的同一份头文件，验证完整高字、一次MRRC、MRC/MCR的物理恢复、19个传输失败点和三类恢复值不匹配，另验证银行事务的1110个故障点、R0恢复／完整DSPSR与DLR变化、37项EL0/EL2成功、41项受限及162项EL1 Unknown，以及VFP的73个故障点、R0/R1恢复、外部当前MIDR/EDSCR与DSPSR/DLR/FPEXC/HCPTR变化、八种保存模式、D16/D32、未使能和原始未知MVFR；严格C警告检查通过。命令检查仅初始化进程内dummy虚拟适配器，保持target未examine，核对七项协议、帮助以及参数和全部23个银行状态的精确原生错误码；所有端口关闭，不连接实际探针/板卡。`tests/encoding.s` 和 `tests/banked-encoding.s` 用GNU Arm汇编器独立确认MRRC、Thumb ISB及银行／当前寄存器编码；`tests/vfp-encoding.s` 核对VMRS／VMOV／HCPTR。

本 VFP 写入后端批次 Linux 候选后端 SHA-256 为 `1dd04e403485c254431ea6f47a3690f2453c682f23b0901ac9d3e321d180ccda`，版本 `0.12.0+dev-gd3ebb8d-dirty (2026-10-05-01:04)`。dirty 来自尚未成为上游提交的适配补丁，构建时间为 UTC。上一批 MRRC／ISB 候选哈希保留在 lock 的 previous_candidates，不能用于当前补丁。候选产物未安装或发布，未来分发须保留源码、补丁与许可，不能把其能力写到现有 Windows 二进制上。

## Windows 后端、依赖和候选包

在 Linux／WSL 中准备主机 GCC、Git、Python 3（tarfile 支持 `filter='data'`）、make、autotools、pkg-config 以及 MinGW-w64 x64 的 gcc、windres、objdump、strip，然后执行：

```sh
sh tools/openocd-adapter/build-windows.sh /tmp/openocd-windows-new
```

脚本不安装主机依赖，不覆盖已有输出路径。`OPENOCD_CROSS_PREFIX` 可指定工具前缀，编译器的目标必须是 `x86_64-w64-mingw32`。固定的 libusb 1.0.30 归档 SHA-256、HIDAPI 0.15.0 和 LibJaylink 提交及许可选择见 `windows-dependencies.lock.json`；Jim Tcl 和 OpenOCD 提交仍由 `source.lock.json` 管理。HIDAPI 的 Windows 单文件嵌入方式已包含描述符重建代码，不能再重复编译该 C 文件。构建隔离 pkg-config 主机库，明确指定 build/host，不借 WSL 的 Windows 程序互操作误判为原生编译。

需要 MinGW 及 GCC runtime 的版权文本；默认从编译器安装前缀的 `share/doc/` 读取 Debian/Ubuntu 文件。其他工具链可在调用 `windows.py stage` 时提供 `--mingw-license` 和 `--gcc-license`。本批使用 Ubuntu 22.04 的 GCC 10 POSIX MinGW-w64 工具链。源码固定不意味着产物逐字节可复现：OpenOCD 构建时间、工具链与路径会影响文件哈希，实际编译器版本及哈希记入 `install/PROVENANCE.json`。

输出包含 OpenOCD、完整脚本、libusb／HIDAPI DLL、两份现有 F429 配置、许可、每个文件的字节数／SHA-256、依赖导入表、`corresponding-source.zip` 和候选运行包。源码包保留完整 OpenOCD／Jim Tcl／LibJaylink／HIDAPI checkout、Git 对象、libusb 原始归档和构建／测试配方。解压源码包后，把它作为脚本的第二个参数，可从这些缓存提交构建全新目录而不下载源代码；主机工具链仍需事先准备。

```sh
sh tools/openocd-adapter/build-windows.sh /tmp/openocd-windows-another /path/to/extracted-source
```

构建检查全部 PE 为 x64、USB/HID 等通道确实编译、静态 DLL 导入依赖闭合；不从 PATH 猜补依赖，不把跨平台编译记为 Windows 原生验证。HID／USB 动态加载的 Windows 系统组件和探针 USB 驱动由宿主提供；离线检查不证明实际驱动兼容。把输出的 `source`、`install`、`corresponding-source.zip` 放到同一个 Windows 输出目录后执行（示例编译器路径需对应本机）：

```powershell
python tools/openocd-adapter/windows.py verify --root G:/Build/openocd-windows-new --cc C:/MinGW/bin/gcc.exe --objdump C:/MinGW/bin/objdump.exe
python tools/openocd-adapter/tests/windows-package.py --candidate G:/Build/openocd-windows-new/install --source-archive G:/Build/openocd-windows-new/corresponding-source.zip --objdump C:/MinGW/bin/objdump.exe
```

原生检查先验证清单、配方和源码ZIP，再执行生产事务测试及真实后端命令检查，核对J-Link、CMSIS-DAP、ST-Link、FTDI的注册，以及两份F429配置和HID／USB bulk后端能离线加载。物理配置加载在config阶段结束，显式init被拒绝；只在前面的独立事务检查中初始化进程内dummy，不连接真实探针或打开调试端口。检查成功后才把 `native_windows_verified` 标为true并重新生成候选ZIP。十一项包检查包含缺失DLL、非PE、同大小篡改、新增DLL、有效ZIP内补丁篡改、Git空目录遗失及独立TCP驱动夹具缺失/变化拒绝。Git refs负向夹具包含有效JSON，避免被新JSON检查提前拒绝而漏验原Git目录问题。

本 VFP 写入后端批次 Windows 候选后端 SHA-256 为 `4e75d9878062b2f33d4377005f1a1bcbc71f4f4aa37ac3f3d8281e91b32afc00`，版本 `0.12.0+dev-gd3ebb8d-dirty (2026-10-05-01:05)`，构建时间为 UTC。本批新目录 Windows 构建、最终修复的增量重编译及原生命令、依赖、配置检查通过；源码 ZIP 固定提交回读已核对，仍不代表探针通信或 R52 实板指令执行通过。现有 `tools/bin/openocd`、依赖锁及安装未改动。最终 tools/profile 合并、安装升级和整个任务完成后的 Release 仍待完成；不能只分发运行 ZIP 而遗漏对应源码与许可。

## 延后执行的物理核心用例

VFP REG-H03 驱动 `scripts/test-register-vfp-hardware.cjs` 默认 4 skipped；独立 `tests/fixtures/register-vfp-board.S` 不写模式／FPU，TCP10=1 或 EN=0 分别停止可选指令。实际 DebugTUI 双核软件流程覆盖 D32、D16、未使能、陷阱受限四类，每类 5 阶段；独立 GNU Arm 钩子编译通过。运行时按每核固件原始参考字核对 S/D/Q 和控制，并验证 peer、状态与工程未变。准备与限制见 [测试说明](../../tests/README.md#开发分支寄存器与内存夹具)；EL1/Guest 合法读取仍未完成，未执行上板。


银行 REG-H02 驱动、独立汇编钩子和逐核案例详见 [测试说明](../../tests/README.md#开发分支寄存器与内存夹具)。默认 4 skipped；Hyp／User 的实际 DebugTUI 二进制软件流程各 5 阶段通过，8 个 R52 模式的独立钩子均汇编通过，不记为上板验证。当前银行的固件样本使用普通 MOV／MRS，其他银行使用独立 GNU 指令，钩子不切换模式或改 FPU 控制；实际试验在 ready 循环暂停，确认每核存储和不可访问项。

`scripts/test-register-timer-hardware.cjs` 默认输出五项 skipped，不连接。专用暂停工程须禁止连接/退出时自动运行、复位或下载，并设置 `on_exit = "disconnect"`。明确提供 `--run --binary EXE --project TOML --core NAME --case JSON` 才执行 REG-H05。`tests/fixtures/register-timer-board.example.json` 是软件占位，需替换实际 MIDR、CVAL、合理差值和独立证据，移除 `software_example` 后才能上板执行。

只读钩子 `tests/fixtures/register-timer-board.c` 记录 CNTPCT/CNTVCT/CNTP_CVAL 的固件 MRRC 样本，不开启 Timer 或修改比较值。各核分别编译固件或将基线改为 per-core 数组，据此修改 case 的变量引用。固件初始化需建立权限、明确 debug freeze 条件，并配置至少一个高字非零的已知稳定 CVAL。驱动按完整 64 位模运算检查基线差值和进展，避免要求不同时间采样相等；检查当前核、稳定控制、选择器、可选 peer 和客户配置未变。GDB 的核心 guard 是逻辑视图，不能单独证明物理 R0/R1 恢复；物理回读由固定适配器内部执行。

真正 ISB 的物理用例复用 `scripts/test-register-selectors-hardware.cjs`：专用固件预先关闭 CP15BEN，工程设置 `isb_command`，分别执行 EL1/EL2 MPU 和 PMU 已知索引，保留 `evidence.synchronization`、原/恢复选择器及控制状态对比。不得为通过用例而启用 CP15BEN；未知结果不自动继续、复位或重试。Timer 驱动已在实际 DebugTUI 可执行文件的双核软件模型六阶段通过，包含独立基线接口和 peer 检查。当前均未上板执行，Linux 编译和软件模拟不记作板卡支持证明。

## 只读 PMU 协议

独立 `aarch64 debugtui_pmu_protocol` 必须返回 `debugtui-armv8-pmu-1 external-identity current-el fresh-count direct-index mrrc64 no-enable no-selector-write stop-on-fault`，不能用通用 MRRC 或 Timer 协议代替。工程显式设置 `registers.pmu_command="aarch64 pmu"`。后端校验外部 AP 的当前 EL2/身份、原生 PerfMon/PMCR.N/HDCR/PMSELR、完整状态/PC 与 scratch 回读，然后返回 22 项 MRC32 或完整周期 MRRC64 及逐次证明。直接事件索引保持 PMSELR；不启动、复位计数或清除溢出标志。低 EL 的 Hyp trap 权限未知时没有数据指令；HDD 禁止当前 EL2 时拒绝。不接受 PMSWINC、事件 4 以上或任意原始命令/写入参数。

生产 C 测试覆盖 23 个手写读编码、1,614 个 I/O 失败点、190 项拒绝、208 项证明/scratch 变化及六个移动周期值。PMU 原批次维护十项文件、六个独立协议；当前 GIC 批次扩展为十一项文件、七个独立协议；Windows/Linux 候选摘要及依赖/GPL/离线命令证据见 [PMU 说明](../../docs/register-pmu.md)。候选未安装、未上板。`scripts/test-register-pmu-hardware.cjs` 默认五项 SKIPPED，十项环境 case 和只读 GNU Arm 固件 hook 已准备，不能由软件测试推导芯片授权/读值。包含 PMU 的旧选择器环境 case 现在也需独立 PMU 协议的当前数量 Probe，不再接受仅凭停止前 Hyp 的物理容量判断。

## GIC 观测协议与接口

独立 `aarch64 debugtui_gic_protocol` 返回 `debugtui-armv8-gic-1 external-identity current-el fresh-capacity physical-icc hyp-ich no-ack no-enable stop-on-fault`。工程显式选择 `registers.gic_command="aarch64 gic"`；默认空值保持原读通道。原生固定允许名单为 43 个 MRC32 编码：29 个 R52 观测项目、12 个已知未实现的 AP 编码和两个被拒绝的 IAR acknowledge 编码。WO 的 EOIR/DIR/SGI 不在允许名单中；ICV AP 是主机从 ICH 同一份 backing 证据派生，不执行 EL2 ICV 指令，也不使用 Guest 栈帧值。

外部 EDSCR/MIDR 先证明当前 AArch32 Debug EL2 和 R52 D13，然后读取并前后核对 DSPSR、DLR、ID_PFR1、ICC_HSRE/SRE/CTLR、ICH_VTR、HCR、ICH_HCR、HSTR 与 R0 物理恢复。R52 CTLR 的物理五位容量和 VTR 的虚拟五位/四列表分别解码；未适配的六/七位值拒绝，不能用停止前 Hyp 或虚拟容量确认物理 AP。低 EL 的重定向/陷阱无法证明时 Unknown 且没有 GIC opcode；已知 HDD 禁止为 restricted。没有使能、acknowledge、EOI/DIR/SGI、解锁或模式写入。LR/LRC 是各一次 MRC32，不拼成 MRRC64。请求任何失败不回退或重试；物理状态/暂存/控制改变即 unknown 和隔离。

生产同一 C 头文件模型覆盖 29 个成功入口、3,190 个 I/O 失败点、239 项安全拒绝和406项状态/暂存变化。Windows/Linux 完整构建、原生命令/独立协议、软件双核 worker 与延后固件证据见 [GIC 自检](../../docs/register-gic.md)。十项环境 case 全部 SKIPPED；`scripts/test-register-gic-hardware.cjs` 默认五项 SKIPPED。未安装或上板，完整 MMIO GIC/Debug 与低 EL ICV 仍未完成。本批同时修正 PMU 安全错误误用 Timer 前缀的问题，由生产 PMU 头文件的三个精确 typed-tag 断言验证；既有独立 PMU 协议保持原字符串。


对应源码ZIP现在包含独立驱动依赖的 `tests/fixtures/register-vfp-write-board.example.json`，PROVENANCE保存其哈希；缺失或变化的示例会使本机包验证拒绝。此前仅从工作区执行测试不能证明单独解压的配方可用。新增 `tests/source-package.py` 在全新目录解压并核对当前配方，脱离工作区重新编译七套生产事务和运行八项TCP驱动测试，可选择用候选执行dummy原生命令；不连接探针。

```sh
python tools/openocd-adapter/tests/source-package.py --archive PATH/corresponding-source.zip --out NEW_DIRECTORY --cc gcc --openocd PATH/openocd
```

EDSCR中的DTR满/空位可能在不同事务间变化；延后驱动每次仍严格验证身份、EL2/AArch32、ITE、故障和TCP10，稳定性比较按执行状态mask进行，原始值完整保留。不能把传输位变化视为EL变化，也不能因忽略传输位而放宽权限检查。

低EL反例同时发现无消息AssertionError曾令驱动的failed标志为false；现始终记录异常类型，新增空异常测试禁止把失败阶段计为成功。
