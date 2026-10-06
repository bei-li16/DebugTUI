# 寄存器功能实现进度

## 2026-10-06：BUS 读取响应边界与显式恢复

GDB/AP 标量、范围与旧 GDB 外设路径共用 Context/状态/通知代次/取消边界，处理 Tcl 阻塞期间积压的 MI 通知；同帧线程切换也拒绝迟到结果。GDB 核对完整连续响应及精确地址，AP 坏响应丢弃连接。成功结果记录实际命令/路线/时间 Access，Watch/Peripherals 请求携带 Context；GDB 已确认 endpoint 与配置提示分开。只读请求断线后仅下一次显式读取重连，选择器/写入恢复失败与既有服务故障继续隔离。见 [BUS 读取边界](bus-read-boundary.md)。

新增六项单元、一项真实 MI 管道集成；最终树分套件验证 **412 单元＋176 集成＝588 通过，2 ignored**。第一次整套 Cargo 586 通过，原生 Memory 暴露只读请求误隔离的问题，修复后独立 Memory 通过。第二次全量 runner 在 600 秒整体预算处终止：其前 15 个测试组有完整摘要，546 通过/2 ignored，最后 write_access 被截断。未把该 runner 标为通过；单独补跑最后 42 项全部通过（62.31 秒），Doc-tests 0 项通过，补齐最终树全部 Cargo 套件。原生 Memory/SVD 均通过；其他 **20 外层套件未选择**。F24 **156/156**、F25 **12/12** 是证据匹配模式数，不是测试数或完整 feature 验收。

Clippy 首次只发现循环形式 lint，按建议改为等价 while-let，随后补测该路径 10 项通过；严格 `cargo clippy --locked --all-targets -- -D warnings` 通过。原始首次 Memory 失败、整套超时、补测和 Clippy 日志均保留。总报告及日志逐字节镜像 [第一次报告](../artifacts/functional-1791246505709-187f9eb2/report.json)、[超时报告](../artifacts/functional-1791247273804-ca49f536/report.json)，[独立写入补测](../artifacts/bus-read-boundary-write-access-final.log)；原始子报告保留在 JSON 引用的 C 盘绝对位置，镜像 Markdown 的相对链接不自动重写。核对 `artifacts/bus-read-boundary-report-mirror.json`，最终分批证据与源码哈希见 `artifacts/bus-read-boundary-final-evidence.json`。

不增加整项勾选，**28 完成／43 未完成**。Watch 地址解析的函数调用限制/线程证明/清理、各面板实际 receipt 展示、全部通道覆盖及完整 BUS 环境驱动仍待完成；其余低 EL、writer、终端视觉、整套验收和 Release 继续推进。目标 active；根据用户最新目标，最终 Release 也基于非主分支。三项补充 [环境 case](../tests/cases/bus-read-boundary.md) 均 SKIPPED，未上板；版本仍为 0.9.3，本轮在 `codex/register-debugging` 提交推送。

## 2026-10-06：REG-406 STM只读配置与独立组件证明

Arm STM v1.1共同身份/功能与STM-500配置区共46项目录，显式chip映射触发有界Probe；可选HWE/DMA需独立同aperture/路线映射和控制类证明。身份、物理路线、owner/代次、上下文及请求次序不依赖CPU MIDR。数据路径始终要求当前Proof/定义地址/实际功能，不写控制、选择器、unlock或stimulus；Trace数据采集解码未接入。详见[STM自检](register-stm.md)。

八项新单元、六项worker/EXE集成覆盖独立组件身份/功能、可选接口、保留编码、四核/Scope All、真实AP大端路线、身份/权限/映射拒绝、peer代次失效和后验变化停止读取；失败旧值保留来源。独立RAM驱动正向及未就绪/身份错误/参考值不符的拒绝路径已验证。软件夹具不证明芯片集成或实板权限；八项环境case全部SKIPPED，默认驱动五阶段SKIPPED；独立C基线仅离线编译。

完整Cargo **406单元＋175集成通过，2 ignored，共581通过**；F24 **156/156**为证据匹配模式数，非用例数。525324 ms无超时；其他**22外层功能套件未选择**。完整报告/Markdown/unit.log逐字节镜像 `artifacts/functional-1791244770256-6c55a69d/`，61份原始子报告保留于总报告引用位置；Node大产物留在C盘，核对 `artifacts/register-stm-report-mirror.json`。严格Clippy通过，日志 `artifacts/register-stm-clippy.log`；目录再生成及后端11项源锁静态核对通过，见 `artifacts/register-stm-static-audit.json`。

仅新增勾选REG-406，**28完成／43待完成**。目标active，完整低EL/其余类别、BUS/writer、完整验收与工具整合/安装/升版/Release仍待完成。源码及既有全局安装仍为0.9.3；本轮提交并推送非主分支 `codex/register-debugging`。

## 2026-10-06：REG-007只读能力矩阵与全类别边界

新增纯缓存 `registers_matrix`，导出类别、精确位宽、全别名依赖/状态条件、选中core/owner、配置endpoint/target/协议和完整观察索引。共享owner在协调器响应边界再次过滤/重算，旧Probe不覆盖配置事实；失败旧值保留原证据。覆盖12个AArch32计划类别，STM明确待补，全部hardware_support保持unverified。配置路线完整不表示后端/协议/权限已通过。详见[能力矩阵](register-capability-matrix.md)；Scope All不广播，disconnected/RUNNING/FAULT导出不进行目标I/O。

七项新单元、四项新集成覆盖全部目录/位宽、条件、完整与旧receipt、MMIO路线、共享代次、实际CLI六项离线配置、原生VFP及故障隔离。延后驱动用实际EXE/MI/TCP/Tcl软件夹具执行四阶段，独立Core1高字值与错误基线拒绝均验证。首轮专项误用不隔离的短响应夹具、随后误用Core0独立高字期望的失败记录保留；修正测试输入，原断言未缩减。

完整Cargo **398单元＋169集成通过，2 ignored，共567通过**；F24 **154/154**是证据匹配模式数，531352 ms无超时，其余22外层套件未选择。总报告/Markdown/unit.log逐字节镜像 [artifacts/functional-1791241698266-d38320e0/](../artifacts/functional-1791241698266-d38320e0/report.json)，56份原始子报告留在F盘，核对 `artifacts/register-matrix-report-mirror.json`。严格Clippy通过，日志 `artifacts/register-matrix-clippy.log`；首轮专项失败记录为 `artifacts/register-matrix-first-failures.json`。固定后端补丁和十一项源码与源锁一致，本轮未修改/重建原生后端，静态核对 `artifacts/register-matrix-static-audit.json`。

只新增勾选REG-007，**27完成／44待完成**；目标active，未标完成或暂停。实际工具/芯片身份、ADS/R52+差异、完整低EL与其余类别、BUS/writer、最终全套验收/安装/升版/Release仍待完成。[八项环境case](../tests/cases/register-matrix.md)均SKIPPED，默认驱动四阶段SKIPPED，未上板。源码/既有全局安装仍为0.9.3，本批提交并推送非主分支 `codex/register-debugging`。

## 2026-10-06：VFP当前Debug状态与独立源码包验证

VFP读写协议升级v2，由选中target的外部MIDR/EDSCR及HALT证明当前EL2/AArch32，保存DSPSR不再作为当前Hyp依据。五项原始访问证明随控制和pair保留，详情区分当前EL与停止前状态；低EL在CPU指令前拒绝，失败保留原值来源。writer每个物理D写入前复核EDSCR，矛盾receipt按未知处理。两项新单元、两项新多核worker测试覆盖八种保存模式、Scope All选中Core1、旧或伪造证明及后续调试；73读/166写故障点和80视图生产模型通过。

Windows/Linux完整重建，七协议/七套生产事务与实际dummy命令、本机DLL/三项离线配置通过。延后TCP驱动八项通过，新增可变化DTR位、低EL零写入和空异常不得误报成功的验证。源码ZIP补齐独立驱动JSON夹具，旧包缺失明确拒绝；新包全新解压后脱离工作区编译七套模型、运行八项TCP及后端命令通过。Windows包拒绝回归十一项通过，保留原九项并新增JSON缺失/篡改两项；原Git refs负向夹具补齐JSON，避免提前失败漏验Git目录。见[VFP证明](register-vfp-proof.md)、[源码自检](register-backend-source.md)。

完整Cargo **391单元＋165集成通过，2 ignored，共556通过**；**F24 149/149**为证据匹配模式数。538354 ms，无超时，其余22外层套件未选择；总报告/日志逐字节镜像 `artifacts/functional-1791238088481-8911e18d/`，原53份子报告留在F盘，镜像核对 `artifacts/vfp-proof-report-mirror.json`。严格Clippy通过，日志 `artifacts/vfp-proof-clippy.log`。首次专项误用单核worker及驱动空异常的失败日志保留，修正后最终验证通过。

本轮不增加TODO勾选，**26完成／45待完成**，目标active；REG-304完整EL1/Guest/User合法读取、FP状态writer及其余TODO、最终全套验收/工具安装/升版/Release仍待完成。[八项环境case](../tests/cases/register-vfp-proof.md)全部SKIPPED，未上板。源码和既有全局安装仍为0.9.3；本轮提交并推送非主分支 `codex/register-debugging`。

## 2026-10-06：S/D/Q存储来源与固定后端源码自检

VFP数据现在保存同次MVFR0/MVFR1/FPEXC、首个D编号及完整128位pair，S/D/Q共享原请求/owner/上下文与区间，失败保留原值/原依据。全部D16/D32视图、能力矛盾/Unknown、特殊浮点/向量、双核Scope All与实际EXE驱动有软件证据。正常测试startup标本D16/D32×大小端四对象离线编译及数据节核对通过，对象未执行；[八项环境case](../tests/cases/register-storage-views.md)保持SKIPPED。详见[存储视图自检](register-storage-views.md)。REG-304完整权限/当前Debug状态仍待完成。

固定OpenOCD源码重新导出、应用补丁及十一项源锁一致；Windows/Linux既有完整构建、七协议/生产事务、本机DLL/CFG与对应源码/运行包证据核对通过。REG-006的源码可获得/可构建范围见[后端源码自检](register-backend-source.md)，不能代替安装或板级能力。

完整 Cargo **389 单元＋163 集成通过，2 ignored，共552通过**；**F24 145/145**为证据匹配模式数，非用例数。完整运行496515 ms，无超时；其余**22外层功能套件未选择**。原始完整报告与53份子报告位于F盘，完整总报告/日志的逐字节镜像为 `artifacts/functional-1791235241817-19eb948a/report.json` 和同目录 `unit.log`；镜像核对见 `artifacts/storage-views-report-mirror.json`。严格Clippy通过，日志 `artifacts/storage-views-clippy.log`。REG-305软件范围与REG-006源码/构建自检已验收，完整TODO为**26完成／45待完成**，目标active；未执行上板、安装或发布Release。

## 2026-10-06：银行读取当前 Debug EL 与原始证明

自检纠正原银行后端以停止前 DSPSR.M 选择当前银行、在低 EL注入MIDR的缺口。新独立协议v2从外部AP取得当前MIDR/EDSCR，每次检查HALT；EL2/Hyp支持三十项，EL0/User支持七项当前User银行。架构将调试态直接CPSR读取定义为受约束不可预测，因此不以MRS CPSR证明模式。EL1中Hyp项受限、其余Unknown，在CPU指令前拒绝，完整EL1银行支持仍未完成。内置目录仍为二十三项，User扩展用自定义目录；旧协议/裸值不回退。

前后身份/执行状态、全位DSPSR/DLR、R0物理保存/恢复/回读均校验。四项原始证明与实际MOV32/MRS32/banked MRS32方法绑定当前owner/context/route/请求区间，在headless和详情可查；失败保留原值和原证明，旧JSON缺字段不制造证据。生产C模型37成功、41受限、162未知、1110故障点；全部三十条GNU编码和八模式只读固件三十参考槽已离线验证。Windows/Linux新目录重建，七协议/事务和Windows本机DLL/离线配置、源码包验证通过。

[八项环境case](../tests/cases/register-banked-proof.md)均SKIPPED；未执行上板、未替换全局安装、未发布Release。**REG-303仍未勾选，24完成／47待完成**。详细范围与反例见[银行当前Debug状态自检](register-banked-proof.md)。

完整 Cargo **385 单元＋160 集成通过，2 ignored，共545通过**；**F24 138/138** 为证据匹配模式数，非用例数。完整运行489102 ms，无超时；其余**22外层功能套件未选择**。报告 `artifacts/functional-1791232474564-90d658f4/report.json`，完整Cargo为同目录 `unit.log`；严格Clippy通过，日志 `artifacts/banked-proof-clippy-final.log`。

## 2026-10-06：R52 新鲜 MMIO 身份/容量与共享证据失效

显式 registers.mmio_probe=true 在现有 Probe 中增加42个有界只读请求，逐步核对外部 MIDR、不同 class 的 Debug/GIC CIDR、GIC IIDR variant/revision、EDDFR 容量、GICD_TYPER 和实际 Debug/GICR affinity。前后 raw/route/aperture/owner/context/请求区间一致才提供物理事实；board base/逻辑 owner 关联仍属配置，64位两字非原子。独立 Probe 样本不会被普通 TYPER 读取覆盖。启用后缺少新鲜证明的数据读取零 I/O拒绝，配置仍单独保留，失败不推断硬件 No。

共享 cluster Proof 在 coordinator 响应绑定请求起始 owner epoch；peer 在探测中或发布后发生生命周期变化时，使共享样本及事实失效，并 FIFO 通知 worker，使后续数据读取拒绝。core私有证明保持本核 context；条件详情新增共享 owner/epoch 和原始完整证明。旧配置默认关闭、旧 JSON兼容。[六项新环境 case](../tests/cases/register-mmio-probe.md)、独立21项只读 C/JSON 和默认五阶段 SKIPPED可执行驱动已准备；未上板，对象仅离线编译。

完整 Cargo **383 单元＋158 集成通过，2 ignored，共541通过**；**F24 134/134**为证据模式数，非用例数。550889 ms，无超时；其余**22外层功能套件未选择**。报告 artifacts/functional-1791228389704-41ff400d/report.json，Cargo为同目录 unit.log；严格 Clippy通过，日志 artifacts/register-mmio-probe-clippy.log。最终驱动新增输入完整性校验后，缺失容量期望在目标I/O前拒绝，实际EXE正向五阶段再次通过，见 artifacts/register-mmio-probe-driver-guards.json。

失败的首次集成编译、未选择核心/夹具错误检查日志均保留于 artifacts/register-mmio-probe-worker-first.log、worker-retry.log、worker-selected.log；修正遵循既有核心选择、目录地址与 Snapshot 旧值来源语义，四项 worker 与一项真实 EXE 延后驱动专项通过，未缩减断言。既有后端十一项源锁、补丁、Windows/Linux候选及对应源码包一致；不修改后端或安装。完成/未完成仍 **24/47**，REG-405/BUS-006 不勾选；完整 Debug/低 EL/STM/Bao、BUS/writer/工具、终端视觉及最终全套验收/升版安装/Release仍待完成，目标 active。

## 2026-10-06：R52 GIC/外部 Debug MMIO 与显式 owner 配置

新增 1,598 项 R52/R52+ MMIO 元数据（GICD 1,485、GICR 33、外部 Debug 80），按 TRM 明确地址、位宽、字段、RO/RW/WO、副作用与实现条件。GICD 属于显式 cluster；GICR/Debug 属于显式 core；GICR SGI/PPI 页为 control base+0x10000。新 component_owners 逐 owner 指定 base/channel/endian，未知/缺失不回退静态默认/别核；读取继续复用现有内存通道与完整 provenance，失败原值保留原来源。新目录要求 owner map，旧目录静态 reader/JSON 仍兼容。

三项单元与三项实际 worker/EXE/MI/TCP 集成通过；四核非连续标签、跨 cluster、64 位不对称字序与实际 target、Unknown/No/WO/手工副作用/错误旧值，以及二十四项独立 RAM 驱动有证据。驱动五阶段正常、四类拒绝和清理已验证；未就绪/错误 owner 零 MMIO，错误身份只读 MIDR。完整 Cargo **379 单元＋153 集成通过，2 ignored，共 532 通过**；**F24 132/132** 是证据匹配模式数，不能当作用例数。整套 497753 ms，无超时；其余 **22 功能套件未选择**。报告 artifacts/functional-1791225291601-66910f0b/report.json，完整 Cargo 为同目录 unit.log；严格 Clippy 通过，日志 artifacts/register-mmio-clippy-final.log。

独立 normal-Hyp 固件离线编译及手册地址/反汇编检查通过，对象未执行；报告 artifacts/register-mmio-firmware-report.json。十二项延后环境 case 和默认五阶段 SKIPPED 驱动已准备，报告 artifacts/register-mmio-hardware-1791224011438-fe09a965/report.json。本批仅核对既有原生候选/十一项源锁/对应源码包一致，不改变后端或安装。

详情见 [MMIO 自检](register-mmio.md)。配置容量仍是配置证据，未冒充新鲜物理 Probe；完整 Debug system route、低 EL ICV/Timer/VFP、STM/Bao、BUS 全项/其他 writer、真实终端视觉、工具整合、最终全套验收、升版安装和 Release 继续待开发。REG-405/BUS-006 未勾选，完整 TODO 仍 24 完成／47 未完成；本轮在非主开发分支提交推送，源码/既有安装基线 0.9.3，目标保持 active。

## 2026-10-06：GIC AP 条件、原生物理/虚拟容量与只读后端

新增独立 `registers.gic_command="aarch64 gic"` 与 `debugtui-armv8-gic-1` 协议。按 R52 TRM/GIC 架构区分物理 ICC、Hyp ICH 及 ICV AP backing：四十三条原生 MRC32 路由中二十九条可观测、十二条额外 AP 在 R52 未实现、两条 IAR 有 acknowledge 副作用而拒绝。目录补齐八项 ICV AP alias 与六项 WO 元数据；LR0–3/LRC0–3 各为独立 32 位，不用 MRRC 或拼装原子视图。未创建 GIC writer，不改变模式/使能/中断状态。

容量不再由停止前 CPSR.Hyp 或 GDB CTLR/VTR 推断。每项从外部 AP 取得当前 MIDR/EDSCR，确认 Arm/D13/AArch32 当前 Debug EL2，逐步校验 ID_PFR1/SRE/HSRE/ICC_CTLR/ICH_VTR、HCR/ICH_HCR/HSTR 及前后 DSPSR/DLR；数据后恢复并物理回读 scratch。真正物理五位与虚拟五位/四列表，各绑定相应原生 CTLR/VTR 成功请求的接口、上下文、owner、源/路由和时间区间。低 EL Unknown、EL2 HDD 已知禁止 Restricted；不回退旧 MRC/GDB，状态/控制/传输不确定即停止并隔离服务。

条件夹具独立验证 ICC/ICH/ICV 的 5/6/7 位边界与未知/已排除零请求；R52 原生只接受五位。Scope All 只读选中物理核，切核/停止点/session/实际线程/frame 变化丢弃旧上下文。取消完成在途恢复后丢弃结果，失败旧值保留原始接口/请求时间。自检补齐 `ProbeBasis.observations` 的完整成功 provenance，No 条目无数据读也能追溯容量的十二项证明；失败不把保留原值/证明冒充成功，旧 JSON 兼容且不制造证据。当前状态/原值依据在详情与 headless 可查。

完整 `node scripts/test-functional.cjs --only unit` 为 **375 单元＋150 集成通过，2 ignored**，共 **525 通过**，**F24 129/129**（匹配模式数），334793 ms 无超时；其余 **22 套件未选择**。报告 [`artifacts/functional-1791220138851-f975cb60/report.json`](../artifacts/functional-1791220138851-f975cb60/report.json)，Cargo 为同目录 unit.log；最终严格 Clippy 日志 `artifacts/register-gic-clippy-final.log`。 新增四项 GIC 相关单元与七项实际 EXE/worker/MI/Tcl 集成，十三项能力回归同步修正。七项覆盖双核 Scope All、原生/alias 全量读、No/WO/IAR、未知/受限、协议/身份/截短/伪造、故障隔离、取消/上下文变化和独立固件延后驱动；驱动正向仅验证单核，双核 worker 隔离另有独立证据。

首轮完整回归保留 `artifacts/functional-1791218850784-f4aaa82d/report.json`：四处旧测试使用 GDB/Hyp 容量推断或脆弱文本搜索。已改用完整 native 证明、按三类 AP 统计，以及真正 TOML writer/write 键校验。下一轮 `artifacts/functional-1791219621548-289ac805/report.json` 在新增 No 依据检查失败：夹具 CTLR=0x403，断言误写 0x400；低两位是合法控制位。已改为精确夹具值，聚焦 `artifacts/register-gic-basis-corrected-test.log` 及最终完整回归通过。失败日志保留，未删除断言或跳过安装验证。

生产 GIC C 模型以四十三条独立编码和十项证明编码验证 **二十九项成功、3,190 个 I/O 失败点、239 项安全拒绝、406 项证明/scratch 变化**。本批还修正前序原生 PMU 安全错误误用 Timer 前缀，由三个精确 typed-tag 断言验证。Linux/Windows 在新 GIC 目录完整重建，七项事务/协议与真实离线命令通过；Windows 本机 DLL/配置/对应源码与九项包拒绝检查通过。当前十一项源锁、补丁/候选散列与源码 ZIP 的逐字节一致性见 `artifacts/register-gic-final-package-audit.json`；构建/原生日志为 `artifacts/openocd-gic-linux-build.log`、`artifacts/openocd-gic-windows-build.log`、`artifacts/openocd-gic-windows-native.log`。原候选/报告保留，未替换全局安装。

GNU Arm 11.4 离线编译独立只读固件，反汇编确认三十七条精确 MRC、二十九条观察值及身份/控制重检，先做 MRS 守卫、无 IAR/MCR/MCRR/MSR/MRRC，见 `artifacts/register-gic-firmware-report.json`；对象未执行。为保留 G: 空间，两份已结束且十四项通过的安装夹具完整归档到 F:，原路径报告/日志按哈希恢复，映射 `artifacts/register-gic-space-archive.json`；本批没有删客户文件或将空间问题计为通过。

只新增勾选 **REG-408 的软件范围**，当前 **已完成／未完成 24/47**。依据用户明确不做上板的要求，[十项 GIC 环境 case](../tests/cases/register-gic.md) 全部准备且 SKIPPED，默认驱动五项 SKIPPED，报告 `artifacts/register-gic-hardware-1791218725523-5f1d83e3/report.json`。完整范围、手册和边界见 [GIC 自检](register-gic.md)。REG-405 的完整 Debug/GIC MMIO/低 EL ICV、REG-401 低 EL Timer 权限、STM/Bao、BUS、其他 writer、工具整合、REG-208 及最终升版/安装/Release 仍未完成；不将软件通过当作硬件访问成功。源码与安装基线仍为 0.9.3，本批提交并推送 `codex/register-debugging`，目标保持 active。



## 2026-10-05：PMU 原生读取、当前 Debug EL 与物理容量

新增独立 `registers.pmu_command="aarch64 pmu"` 和 `debugtui-armv8-pmu-1` 协议。R52 适配二十二项 32 位控制/事件寄存器及完整 64 位 PMCCNTR；直接索引保留 PMSELR，周期只用一次 MRRC，不自动开始、清空或配置计数。按 TRM 区分 SEL=31 的 PMXEVTYPER 周期过滤器别名和 PMXEVCNTR 的 UNDEFINED；未知索引在数据指令前拒绝。目录补齐位域、混合权限、注释及实现条件，R52/R52+ 同步生成；D16 身份和低 EL 完整权限仍未适配。

后端逐项读取外部 Debug AP MIDR/EDSCR 与当前 Debug EL，保存前后 DSPSR/DLR、ID_DFR0、PMCR、HDCR、PMSELR 及 scratch 恢复回读。当前 Arm/D13/AArch32 EL2 才确认物理四项容量；EL0/EL1 返回 Unknown，不改变模式/控制位或用停止前 CPSR 猜测陷阱权限。失败不重试、不回退，未知物理状态隔离服务。新 `Access.pmu` 与完整原生宽度/方法、主机请求区间、选定 owner/context 一起保存；失败旧值保留旧证据，跨条目/跨核不视为同时采样。

自检修正旧 Probe 仅凭停止前 Hyp 与 PMCR.N=4 确认物理容量的问题。现在要求同次原生 PMCR 读取的来源、视图、响应阶段、上下文、实际 MIDR、匹配值和当前 Debug EL2 证据；旧原始 `pmu.pmcr_n` 仍可显示，HDCR.HPMN 另记且不授予 Guest 访问。原生 C 错误路径另修正为成功后才复制 scalar，避免失败时使用未初始化值；Windows/Linux 在新的 final 目录重建，旧候选与证明保留。

最终 `node scripts/test-functional.cjs --only unit` 为 **371 单元＋143 集成通过，2 ignored**，共 **514 通过**，**F24 127/127**，370979 ms 无超时；其余 **22 套件未选择**。报告 [`artifacts/functional-1791213732352-d18b6c32/report.json`](../artifacts/functional-1791213732352-d18b6c32/report.json)，Cargo 为同目录 unit.log；严格 Clippy 日志 `artifacts/register-pmu-clippy.log`。新增两项 PMU 单元、一项新鲜能力证据单元及六项真实 EXE/worker/MI/Tcl 集成，含双核 Scope All、全部宽度、协议/身份/权限/伪造/截短拒绝、取消、context 变化、故障隔离及旧值来源。延后驱动执行独立固件基线、未就绪、参考值不同与计数已开启四种软件流程。

首轮完整回归失败报告 [`artifacts/functional-1791212601141-051d7e49/report.json`](../artifacts/functional-1791212601141-051d7e49/report.json) 保留：旧 selector 驱动依赖停止前 Hyp 的容量推断。夹具改为显式新鲜 PMU 协议，同时校正模型直接索引与原 selector 值一致；原选择器事务及恢复断言保留，聚焦 `artifacts/register-pmu-selector-driver.log` 与最终整套均通过。为保留空间，三个已结束且十四项通过的安装夹具完整迁往 F: 任务归档；原路径报告/日志恢复并逐字节校验，映射 `artifacts/register-pmu-space-archive.json`。本轮没有空间失败或删除客户文件。

最终生产 C 验证 **23 项独立手写编码、1,614 个 I/O 失败点、190 项安全拒绝、208 项证明/scratch 变化、六项完整 64 位移动计数与 LC/E 组合**。Windows/Linux 新目录构建、实际离线命令、Windows 原生依赖/配置验证及九项包拒绝测试通过；对应源码包含当前十项源文件、补丁、测试与 README，散列一致。日志为 `artifacts/openocd-pmu-linux-final-build.log`、`artifacts/openocd-pmu-windows-final-build.log`、`artifacts/openocd-pmu-windows-native-corrected.log`、`artifacts/openocd-pmu-package-corrected-test.log`，最终 Windows 证明 `.dev/openocd-windows-pmu-final/windows-tests/report.json`；一致性自检 `artifacts/register-pmu-final-package-audit.json`。GNU Arm 的只读固件离线编译/反汇编确认 22 项 MRC 与一项 MRRC，先检查 CPSR、无 MCR/MCRR/MSR，见 `artifacts/register-pmu-firmware-report.json`；对象未执行。

只新增勾选 **REG-404**，当前 **已完成／未完成 23/48**。按任务不执行上板，[十项 PMU 环境 case](../tests/cases/register-pmu.md) 均已准备且 SKIPPED；默认驱动五项 SKIPPED，报告 [`artifacts/register-pmu-hardware-1791213796124-dd11aab0/report.json`](../artifacts/register-pmu-hardware-1791213796124-dd11aab0/report.json)。完整范围及候选 SHA256 见 [PMU 自检](register-pmu.md)。REG-401 的低 EL 使能/权限、GIC/Debug/STM、Bao 验证、BUS、writer、工具整合、REG-208 和最终升版/安装/Release 仍待完成。源码与安装基线仍为 0.9.3，本批交付位于非主分支 `codex/register-debugging`，目标保持 active。

## 2026-10-05：Timer 单项完整读取与请求时间界限

新增 `Access.completed_ms`，分别在完整 TCL 帧和匹配 MI token 的结果记录到达后写入；与已有实际 dispatch 起点共用当前 worker 的单调时钟。未发送/断帧/旧 JSON 不制造终点。新 Timer 的 `read_method` 严格绑定原生宽度与一次 MRC32/MRRC64，旧证据缺失时为 Unknown；失败保留原值的完整时间/物理来源，详情与 headless 可查。单项 MRRC 复制同一值到 Rt 低字/Rt2 高字，再从 pair 传输取回；不把请求中点/响应时刻当硬件时间戳，跨条目/跨核不能视作同时采样。手册依据为 R52 TRM 表 11-1、§4.2.18 和完整 DDI0487 M.b F5.1.117（PDF 12139–12140）。[架构](../ARCHITECTURE.md) 与 [Timer 自检](register-timer.md) 已同步。

新增一项单元覆盖六项极限 u64、原生方法/位宽与旧证据 roundtrip；原有十五项双核测试核对请求区间/方法，失败来源保留测试核对原始完整 access。新增一项 Rust 集成启动实际 EXE/MI/Tcl，执行全 64 位回绕与低字进位、允许冻结，以及冻结必须进展/倒退/异常高字/超过固件窗口四种失败流程。合法两流程各六阶段通过，四个失败在 COUNTERS 阶段拒绝并正常清理；target/配置保持、无回退。两个既有传输边界测试同时验证完整帧/拒绝/断帧的终点语义，F24 新增四个模式，完整证据 **118/118**。

生产 C 测试增加六项×八个边界值共 **48 个动态回读**，模型 Timer 在物理 R0/R1 回读之间前进，输出仍等于单次指令复制的原值、暂存恢复完整。Windows/Linux 同一生产头文件测试、真实 dummy 命令及 Windows 原生包/依赖/配置验证通过；后端源码/补丁和候选二进制未改变，当前源码/测试重新封装。证明为 `artifacts/openocd-timer-coherence-linux/report.json`、同目录 `timer-transfer-test.log`、`artifacts/openocd-timer-coherence-windows-native.log` 和 `.dev/openocd-windows-timer/windows-tests/report.json`；没有新后端重编译、实际探针或目标指令执行。

完整 `node scripts/test-functional.cjs --only unit` 为 **368 单元＋137 集成通过，2 ignored**，共 **505 通过**，**F24 118/118**，292198 ms 无超时；其余 **22 套件未选择**。最终报告 [`artifacts/functional-1791208232338-ede8d32d/report.json`](../artifacts/functional-1791208232338-ede8d32d/report.json)，完整 Cargo 为同目录 unit.log；严格 Clippy 通过，日志 `artifacts/register-timer-coherence-clippy.log`，聚焦证据 `artifacts/register-timer-coherence-focused.log`。首轮在隔离安装复制 EXE 时因 G: ENOSPC 失败，失败 Cargo 日志 `artifacts/functional-1791207380838-4751fdcf/unit.log` 与 runner 错误 `artifacts/register-timer-coherence-enospc.log` 保留；当轮 JSON 未写完，不计为通过。删除生成缓存/重复副本被自动审批阻止后，改为完整归档三个已结束的缓存/安装夹具目录到 F:，恢复约 5.46 GiB，已完成测试报告/日志仍在原位置且与归档逐字节一致；位置映射 `artifacts/register-timer-coherence-space-archive.json`。最终整套重新运行通过，没有用跳过安装测试掩盖空间错误。

只新增勾选 **REG-403**，当前 **已完成／未完成 22/49**；按本任务要求不执行上板，[十四项环境 case](../tests/cases/register-timer.md) 均已准备且 SKIPPED，默认驱动仍五项 SKIPPED，报告 [`artifacts/register-timer-hardware-1791208752908-504a502c/report.json`](../artifacts/register-timer-hardware-1791208752908-504a502c/report.json)。REG-401 的低 EL 权限、其余模块/工具整合/writer、REG-208 及最终版本/安装/Release 仍未完成。源码与安装仍为 0.9.3，本批交付位于非主分支 `codex/register-debugging`，目标保持 active。


## 2026-10-05：专用 Timer 生产后端、当前物理 Debug 证据与最低协议

新增显式 `registers.timer_command="aarch64 timer"` 与独立协议；按精确 CP15 编码路由十五项，旧配置保留原通道，选用后端后不回退 GDB/旧 MRC/MRRC。依据完整 R52 TRM、DDI0568A.c 以及工作区 DDI0487 M.b H2.4.5/H2.4.8，系统寄存器保持当前 EL 权限与 trap，HDD=0 不表示豁免。当前 core 的外部 AP MIDR/EDSCR 提供身份与 Debug EL，停止 DSPSR 不代替当前 EL。实际 Arm D13/AArch32 的 EL2 支持全部十五项；EL1 支持频率、CNTKCTL 和四项 CNTV，其他未证明使能分别保留 Unknown 或明确拒绝，不改变模式/控制位。完整 EL0、EL1 物理权限与 R52+ 身份仍未完成。

六项六十四位各执行一次真正 MRRC；前后完整 DSPSR/DLR、外部身份/EDSCR 与 R0/R1 恢复回读检查通过才发布结果。物理证据绑定各次值来源，在 headless/详情/旧值来源中保留。两项新增单元和五项真实协调器/MI/Tcl 集成覆盖全部宽度/高字、Scope All 选定核、协议/身份/权限/截断/伪造拒绝、失败旧值证据、取消丢弃、实际线程/帧变化与恢复未知 FAULT；没有隐式重试或后续不可靠访问。REG-H05 实际 EXE 分别执行旧 MRRC 与新 Timer 双核软件流程，各六阶段通过；新 case 记录实际物理证据，仍用独立固件符号核对高字与计数器窗口。软件流程未执行目标指令。

生产 C 覆盖十五项独立指令字、474 个失败点、12 个 EL1/HDD 合法组合、48 个权限拒绝及完整状态/PC/身份变化。Windows/Linux 新目录构建、真实离线命令及 Windows 原生依赖/配置/源码包验证通过；固定补丁 SHA256 `7cf862cde188de89d668c25031cb724030291a4fafb59bbb24952197b4958c69` 与全部九份源码和候选一致。最终证据为 `artifacts/openocd-timer-linux-build.log`、`artifacts/openocd-timer-windows-native-final.log` 与 `.dev/openocd-windows-timer/windows-tests/report.json`；候选未安装或发布，`board_support_verified=false`。

完整 `node scripts/test-functional.cjs --only unit` 为 **367 单元＋136 集成通过，2 ignored**，共 **503 通过**，**F24 114/114**；其余 **22 套件未选择**，344536 ms 无超时。报告 [`artifacts/functional-1791205816814-cf34d3cc/report.json`](../artifacts/functional-1791205816814-cf34d3cc/report.json)，完整 Cargo 为同目录 unit.log；新增来源路径后当前 F24 manifest 的证据模式保持一致，补充报告 `artifacts/register-timer-adapter-f24.json`。严格 Clippy 通过（`artifacts/register-timer-adapter-clippy.log`）；首次仅发现测试多余借用，失败保留 `artifacts/register-timer-adapter-clippy-before.log`，修正后严格检查与两项单元复测通过，未改变生产行为。早期夹具配置及分组断言错误记录也保留，最终聚焦证据 `artifacts/register-timer-adapter-focused.log`。

按本任务不执行上板的要求，[十三项环境 case](../tests/cases/register-timer.md) 均已准备且 SKIPPED；默认驱动仍五项 SKIPPED，报告 [`artifacts/register-timer-hardware-1791205849895-967ff399/report.json`](../artifacts/register-timer-hardware-1791205849895-967ff399/report.json)。仅新增勾选 **REG-402**，当前 **已完成／未完成 21/50**；REG-401/403、其余系统/工具整合/writer、REG-208、最终升版/安装/Release 仍未完成。完整范围见 [Timer 自检](register-timer.md)。本批交付位于 `codex/register-debugging`，目标继续 active，源码和既有安装仍为 0.9.3。


## 2026-10-05：Timer 独立固件基线与 Debug state 权限边界

自检发现正常执行 EL 权限不能直接作为 Debug state 读取门禁：完整 DDI0568A.c F1.3.3–4 区分 EDSCR.HDD 与 Hyp invasive debug 授权，HDD=1 时低 EL 的 EL2 trap 可成为 UNDEFINED。目录访问说明现明确正常执行规则与后端 Debug state 证据的区别，不用停止前 CPSR 或配置值猜测通用读取权限。完整运行时权限适配仍未完成。

专用 REG-H05 基线驱动先核对 Hyp 与 ready，再访问选定核/peer 控制；该要求限定正常执行固件基线，不代替应用的通用权限判断。独立固件先 MRS 检查 Hyp，再只读采样十五项 Timer；四个 CVAL/offset 有独立符号，驱动分别比较符号、期待值和完整 MRRC 值，两个动态计数器按窗口核对。实际 EXE 双核软件基线六阶段通过；新增一项 Rust 集成执行 SVC、未就绪、独立基线不同三种失败流程，验证正确 STOP/WIDTH 阶段与正常清理，前两类零 TCL 访问。最初单核夹具误带多核 control_scope 的失败保留为 `artifacts/register-timer-baseline-fixture-before.log`，修正配置后保留完整断言通过；聚焦日志 `artifacts/register-timer-baseline-focused.log`。

GNU Arm 11.4.0 的离线 R52 对象以严格 C 警告编译通过；实际反汇编与独立手写九项 MRC、六项 MRRC 编码一致，MRS 在前，无 MCR/MCRR/MSR，未执行目标代码。源/对象/反汇编 SHA256 与限制记录在 [`artifacts/register-timer-firmware-report.json`](../artifacts/register-timer-firmware-report.json)。默认硬件驱动仍是五项 SKIPPED，报告 [`artifacts/register-timer-hardware-1791202201841-d4a6e6af/report.json`](../artifacts/register-timer-hardware-1791202201841-d4a6e6af/report.json)。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **365 单元＋131 集成通过，2 ignored**（共 **496 通过**），**F24 107/107**；其余 **22 套件未选择**。报告 [`artifacts/functional-1791201882470-b0adc186/report.json`](../artifacts/functional-1791201882470-b0adc186/report.json)，完整 Cargo 日志为同目录 `unit.log`；整套 328333 ms，无超时。严格 Clippy、目录重新生成比对、格式与差异检查通过，日志 `artifacts/register-timer-baseline-clippy.log`。

具体范围见 [Timer 自检](register-timer.md)，[十二项环境 case](../tests/cases/register-timer.md) 均 SKIPPED，当前 **已完成／未完成 20/51**，本批未新增勾选。REG-401/402/403 的完整 Debug state/EL1/Guest/User 权限、实际后端及采样一致性、REG-208、其余系统/工具整合/writer 和最终升版/安装/Release 仍待完成。提交并推送 `codex/register-debugging`，未上板、修改全局安装或发布，目标保持 active。


## 2026-10-05：Timer 十五项目录与 Unknown 读取约束

按 R52 TRM 表 11-1、完整 Armv8-R supplement 表 E1-1 和基础 Timer 字段定义，补齐九项三十二位与六项六十四位的描述、访问说明、字段及 `timer.present=1` 条件。Unknown 不自动读取，No 手动也不请求；实际 Probe 的观察和原声明分别保存。TVAL 明确为三十二位有符号差值；ENABLE=0 时 TVAL/ISTATUS 为架构 UNKNOWN，保留原始位但不提供误导的 ISTATUS 枚举。字段访问可覆盖整寄存器，全部六十四位高低位保留，未增加 Timer writer。

新增三项单元与一项实际协调器/MI/Tcl 集成。独立编码和不对称数据覆盖全部十五项，Scope All 仅请求 core1；验证声明覆盖、Unknown/No 零请求、完整高字、单项未知错误隔离、Status 原值/条件依据保留、新 stop 失效及旧 context 拒绝。生成器 `--check` 在临时目录比对三份目录，M4 未改变。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **365 单元＋130 集成通过，2 ignored**（共 **495 通过**），**F24 106/106**；其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791200193342-d937fe7e/report.json`](../artifacts/functional-1791200193342-d937fe7e/report.json)，完整 Cargo 日志为同目录 `unit.log`；整套 283800 ms，无超时。严格 Clippy、格式和差异检查通过，Clippy 日志 `artifacts/register-timer-clippy.log`；聚焦记录为 `artifacts/register-timer-model.log`、`artifacts/register-timer-worker.log`、`artifacts/register-timer-gdb.log`。

失败证据保留：旧 Unknown 自动读取回归 `artifacts/register-timer-before.log`；新集成的状态分类/响应与快照/等待停止三项测试开发失误分别为 `register-timer-worker-before.log`、`register-timer-worker-response-before.log`、`register-timer-worker-stop-before.log`。首次完整回归 [`artifacts/functional-1791199829590-2342083e/report.json`](../artifacts/functional-1791199829590-2342083e/report.json) 发现旧 GDB 按需测试未声明 Timer 实现，新增条件正确阻止读取；该已知可读夹具补上显式声明后保留原全部请求/位宽/失效断言，重跑完整通过。没有把失败计为通过，也未改变运行时代码迎合测试。

范围见 [Timer 自检](register-timer.md)，[十二项环境 case](../tests/cases/register-timer.md) 均 SKIPPED。REG-401/402/403 的完整 EL1/Guest/User 权限、全部独立固件基线、实际后端及一致性验收仍未完成，本批未新增勾选，当前 **已完成／未完成 20/51**。前序框架、配置/Setup、生命周期、共享归属、读取来源与条件依据一并回归。其余系统/工具整合、REG-208 实际终端、writer、最终升版/安装/Release 继续待完成。提交并推送 `codex/register-debugging`；未上板或发布，源码版本仍为 0.9.3，目标继续 active。


## 2026-10-05：可选条件、别名继承与持久判定依据（REG-110）

新增结构化 `eligibility`，保存全部 min/max 条件、声明值与当前观察、目录/CPU/架构/来源、原始能力依据及 session/stop/core/frame。成功读取不丢失条件，失败旧值的原依据另存 `last_value_eligibility`，连续失败及共享拒绝不能覆盖；旧生产者明确 Unknown。别名继承全父链条件、WO 和副作用策略，修复 Unknown 父项被子项自动读取；WO 即使 manual 也不发送。Status 只读详情在 45×12、80×24、120×36 逐页核对，不新增 Probe/read。

手册核对修正 PMCR.N 的模式含义与旧夹具数量：EL0/EL1 可返回 HDCR.HPMN，原值不能证明物理数量；适配 R52 只在实际 Hyp N=4 时发布物理 count。R52 物理 ICC 只接受 TRM 规定的五位，未适配值保留原字段但容量 Unknown；不复用虚拟 ICH 容量。通用 5/6/7 条件边界仍为软件模型，完整 CPU/类别适配继续留在后续任务。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **362 单元＋129 集成通过，2 ignored**（共 **491 通过**），**F24 102/102**；其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791195856192-bfc9079e/report.json`](../artifacts/functional-1791195856192-bfc9079e/report.json)，完整 Cargo 日志为同目录 `unit.log`；整套 313625 ms，无超时。严格 Clippy、格式及差异检查通过，Clippy 日志 `artifacts/register-eligibility-clippy.log`。本批新增九项 Rust 单元、四项集成；其中一项集成执行四项确定性 Node 生命周期测试。

自检保留两个失败证据：旧别名自动读取回归 `artifacts/register-eligibility-before.log`；首次完整回归 [`artifacts/functional-1791194988226-c34ab082/report.json`](../artifacts/functional-1791194988226-c34ab082/report.json) 在 VFP 软件驱动清理阶段失败。测试助手原先在 child exit 时提前拒绝尚未排空的退出响应，现等待 close，缺少响应、错误响应与非零退出仍失败。确定性修复前/后日志为 `artifacts/register-eligibility-session-before.log`、`artifacts/register-eligibility-session.log`；原 VFP 四种流程重跑通过，见 `artifacts/register-eligibility-vfp-cleanup.log`，最终完整回归也通过，没有把失败计为通过。

逐项范围见 [条件依据自检](register-eligibility.md)，[八项环境 case](../tests/cases/register-eligibility.md) 均 SKIPPED。本批仅新增勾选 REG-110，当前 **已完成／未完成 20/51**。前序目录交付、配置/Setup、生命周期、共享归属、读取来源、取消及 writer 一并回归。REG-001–008、REG-208 实际终端视觉、各新增系统类别/合法模式/writer、完整工具整合、最终升版/安装/Release 仍待完成。提交并推送 `codex/register-debugging`；没有上板、修改系统全局安装或发布，源码和安装基线仍为 0.9.3。

## 2026-10-05：寄存器目录打包、初始化与升级保留（REG-107）

生产 npm 打包现在检查 install.cjs、devices.toml 和三份 M4／R52／R52+ 目录；ZIP 解压后逐项核对同一载荷的 SHA256。CLI 帮助明确 `--init-profiles` 创建 chip catalogue 与用户寄存器扩展目录、保留客户文件。新增扩展路径被客户普通文件占用时的失败测试，确认原 devices 与占位文件逐字节保留。内置目录编译进 EXE，初始化保持用户扩展目录为空，避免默认文件遮蔽后续版本；随包模板与实际内置 API 的完整 Catalogue 逐字段一致。

新增实际生产打包与隔离 npm 生命周期集成测试，十四项 case 全部通过。覆盖中文／空格路径、npm／直接 EXE／ZIP、真实 postinstall、CMD／PowerShell 入口、core.0/core.2 的 builtin/user 来源、重复安装／卸载／重装、损坏文件／同名目录 override，以及初始化失败；所有客户目录、profile 和工程均保留。实际 v0.9.3 旧包经发布方 SHA256SUMS 校验后，在 private prefix 单独验证同样十四项流程。旧 EXE SHA256 为 `03e7ab0f2d78df9becc16b7cda2e92fc67b5518eaf880842b942af7ef9f47950`，当前优化 EXE 为 `4337c43302cfed600e9a5db0d8b58d2d390ee87c013040ab9fcd4dd45087880b`；EXE 与 ZIP 替换均记录二者。代码仍标 0.9.3，这份真实历史证据证明同版本不同内容替换；模拟 0.0.0-fixture 另行覆盖 npm 版本变化，正式升版后的真实版本升级与公网交付仍需 REG-505。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **353 单元＋125 集成通过，2 ignored**，**F24 89/89**；其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791190761680-72bc13d9/report.json`](../artifacts/functional-1791190761680-72bc13d9/report.json)，完整 Cargo 日志为同目录 `unit.log`。本轮隔离安装报告为 [`artifacts/register-distribution-1791190800978-a7964c3b/report.json`](../artifacts/register-distribution-1791190800978-a7964c3b/report.json)，真实旧包报告为 [`artifacts/register-distribution-1791188047310-1d153822/report.json`](../artifacts/register-distribution-1791188047310-1d153822/report.json)。严格 Clippy 与优化构建通过，日志分别为 `artifacts/register-distribution-clippy.log`、`artifacts/register-distribution-build.log`。此前整套回归达到旧 300 秒限时，被 runner 终止，失败报告 `artifacts/functional-1791188172267-5915d23c/report.json` 保留；新增顺序打包集成约需 150 秒，因此整套限时调整为 600 秒，各子进程仍保留自身限制。最终全套 273805 ms 结束且没有超时，没有把前一次中断计为通过。

逐项范围见 [目录交付自检](register-distribution.md)，[八项环境 case](../tests/cases/register-distribution.md) 均 SKIPPED。前序配置、Setup、生命周期、共享 owner、读取来源与取消回归继续通过。该历史批次仅勾选 REG-107，结束时 **已完成／未完成 19/52**。当时 REG-110、REG-208 实际终端视觉、R52+ 实际身份及完整可选类别、其余系统／writer、完整工具集、最终升版／安装／Release 仍未完成。本轮提交并推送非主分支；没有上板、修改系统全局安装或发布 Release，安装版本仍为 0.9.3。


## 2026-10-05：Setup CPU／目录预览与不匹配提示（REG-108）

增加 CPU、文件及未提交候选的 F1 只读可滚动详情，显示目录名称、架构、来源、说明、访问条件与 capability fact 范围；配置 CPU、Chip 关联与 Observed CPU 分别展示。M4／R52／R52+、用户 preset、Automatic、GDB 与文件引用的取消／保存经隔离子进程矩阵验证，不改写客户 profile、设备及目录，也不改变核心／backend／启动／build／download 设置。修复 CPU picker 的 F2 错转项目浏览、错误文件关闭浏览器，以及文件来源无 CPU preset 时误显示 GDB 列表。当前身份仅在 STOPPED 且 session／stop／core／frame 全部相同、草稿目标／实际访问路由不变时可用；其他核心或未适配 MIDR 保持 Unknown。只读预览与 Registers status 不提交 probe/read。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **352 单元＋124 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **87 个证据模式全部通过**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791185248280-a74b4bef/report.json`](../artifacts/functional-1791185248280-a74b4bef/report.json)，完整 Cargo 日志为同目录 `unit.log`，Clippy 日志为 `artifacts/register-setup-clippy.log`。本批新增七项单元测试；逐项范围见 [Setup 目录选择自检](register-setup-catalogues.md)，[十二项环境 case](../tests/cases/register-setup-catalogues.md) 均 SKIPPED。完整回归继续核对前序配置、生命周期、共享归属与读取来源，没有替代或缩小其要求。该历史批次仅新增勾选 REG-108，批次结束时 **已完成／未完成 18/53**。REG-107/110、REG-208 实际终端视觉、R52+ 实际身份及完整可选类别、其余系统与 writer、全部工具交付、最终安装／Release 仍未完成。本轮提交并推送非主分支；没有上板、安装或发布，安装版本仍为 0.9.3。

本文件记录开发分支上的实际实现，配合 [开发 TODO](registers-development-todo.md) 使用。当前仍是未发布的开发版本；下述软件验证不能作为芯片或 OpenOCD 实板能力证明。

2026-10-05 配置与芯片关联批次（REG-102）：自检修复芯片默认 CPU 覆盖 profile 明确选择的问题，选定 backend/profile 与项目的显式 CPU/目录（包括空字符串）先于芯片默认；用户同名 override 是目录、不可读或失效链接时报告错误，只有路径不存在才回到内置。新增严格嵌套 schema 与完整配置往返、九组选择矩阵、tools.root/显式环境/project 路径及中文/空格、profile_dir、组件局部继承测试。实际 EXE 十项独立 case 覆盖旧 GDB 回退、全部配置层、用户目录、错误与 core.0/core.2 独立上下文；不启动 GDB/Probe、不修改客户原文件。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **345 单元＋124 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **80 个证据模式全部通过**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791181559212-d0358482/report.json`](../artifacts/functional-1791181559212-d0358482/report.json)，完整 Cargo 日志为同目录 `unit.log`；Clippy 日志为 `artifacts/register-config-clippy.log`。实际 EXE 十项 case 报告为 `artifacts/register-configuration-1791181607036-1b19436a/report.json`。本批新增四项单元、一项集成，逐项证据见 [配置自检](register-configuration.md)，[八项环境 case](../tests/cases/register-configuration.md) 均 SKIPPED。该历史批次仅新增勾选 REG-102，批次结束时 **已完成／未完成 17/54**。REG-107/108/110、终端视觉、其余系统与 writer 类别、完整工具交付、最终安装与 Release 仍待完成；本轮提交并推送非主分支，未上板或发布，安装版本仍是 0.9.3。

2026-10-05 读取来源批次（REG-210）：八类目录 reader 与 Probe／MPU／selector 分开保存目录声明和实际值请求。类型化 provenance 记录实际 GDB 名称／稀疏索引或 TCL endpoint／请求 target，MMIO 补齐配置来源、channel、地址、布局字节序、bus width/count 与非原子性。阶段与时间从实际发送／完整响应位置取得，服务拒绝不能冒充发送，完整错误响应不能冒充有效值。嵌套 alias 与重叠 VFP pair 保留首次父请求；Console 后新 GDB endpoint 保持 Unknown，内部固定 cache flush 成功后保留连接证据。最新失败与旧值来源独立保存，旧来源缺失时保持 Unknown，整条 selector 请求报错不把旧来源改成最新尝试。Status 在宽窄窗口展示目录／配置／当前观察 CPU、Scope／Owner、路由及保留来源，不显示内部协议脚本，查看不访问目标。

自检前序 REG-109 时，独立四核字节夹具复现“共享新值被丢弃 → 随后刷新失败 → 中间快照带回被丢弃字节”的遗漏。协调器现在把未获认可的非 Valid 快照同样限制为该路由最后已认可的原值与来源；新增断言覆盖中间 Snapshot、Response 和随后 frame/status Snapshot。失败复现日志 `artifacts/register-provenance-rejected-refresh.log` 与修复后 `artifacts/register-provenance-shared-fixed.log` 均保留。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **341 单元＋123 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **74 个证据模式全部通过**，其余 **22 个功能套件未选择**。最终报告为 [`artifacts/functional-1791180522905-29b4ab48/report.json`](../artifacts/functional-1791180522905-29b4ab48/report.json)，完整 Cargo 日志为同目录 `unit.log`，Clippy 日志为 `artifacts/register-provenance-clippy.log`。新增六项单元、一项实际 MI/TCP 集成，强化前序全部读取家族与共享域测试；逐项证据见 [读取来源自检](register-read-provenance.md)，[12 项环境 case](../tests/cases/register-read-provenance.md) 均为 SKIPPED。该历史批次仅新增勾选 REG-210，批次结束时 **已完成／未完成 16/55**。完整配置／升级交付矩阵、实际终端视觉、其余系统及 writer 类别、全部延后驱动、最终安装和 Release 仍未完成。本轮提交并推送非主分支 `codex/register-debugging`，没有上板或发布；安装版本仍为 0.9.3。

2026-10-05 共享归属批次：核对 percore／percluster／perchip 元数据及显式拓扑，`registers_list` 返回实际配置拓扑及来源。未知 cluster／chip 不猜测、不读取；身份、映射数量和 alias scope 严格校验。UI 缓存加入采样核心，保留各 worker 路由；协调器按每个共享 owner 维护独立代次，相关成员生命周期变化使其 cluster／chip 失效，其他 cluster 和私有样本保留。未映射 producer 保守使所有声明共享域失效。读取期间 peer 活动使对应共享新值丢弃，保留此前原值及时间；新 session 首次失败不恢复旧会话原值。worker 无响应时立即发布相关 owner 失效；状态仍为 STOPPED 的新停止通知立即发布失效，兼容 GDB 投影也在首个通知中同步过期，不追加读取。界面校验 owner 代次、选择性清除失败退避；100×24／35×12 键盘帮助核对 owner、采样核心、代次、来源及未知归属。

完整 `node scripts/test-functional.cjs --only unit` 为 **335 单元＋122 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **69 个模式全部有通过证据**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791176224477-eaabe71a/report.json`](../artifacts/functional-1791176224477-eaabe71a/report.json)，完整 Cargo 日志为同目录 `unit.log`，Clippy 日志为 `artifacts/register-shared-clippy.log`。本批两项模型、一项故障边界、一项 UI 和五项四核实际管道集成的逐条自检见 [REG-109 共享归属](register-shared-owners.md)；[八项人工 case](../tests/cases/register-shared-owners.md) 均为 SKIPPED。该历史批次仅新增勾选 REG-109，批次结束时 **已完成／未完成 15/56**；当时 REG-210 的完整实际路由、配置／升级交付矩阵、终端视觉、其他系统及 writer 类别和最终 Release 仍未完成。安装版本仍为 0.9.3，本轮未安装、发布或上板。

2026-10-05 缓存生命周期批次：新增显式 selected_frame／physical_core 采样语义，多层 alias 继承根 reader；无 MRRC 配置时来源记录实际 GDB fallback。实际 GDB 线程／帧变化使整批结果丢弃，不覆盖最近有效原值和时间。切帧及返回原帧不会复活旧值；Reset 和任意 Console 命令发送前清除相关名称缓存、Probe、草稿并使样本失效，共享复位及多核 Console 先使全部受影响 worker 失效，命令仍仅发送一次。复位部分失败、符号索引替换、断开后更换 ELF、双核切换及重连、Scope All 不广播与 UI 迟到回复已有实际管道验证。

完整功能入口 unit suite 通过：**331 单元＋117 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **60 个模式全部有通过证据**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791171328667-720b123c/report.json`](../artifacts/functional-1791171328667-720b123c/report.json)，完整 Cargo 日志为同目录 `unit.log`；Clippy 日志为 `artifacts/register-lifecycle-clippy.log`。详见 [REG-106 自检](register-cache-lifecycle.md) 及 [十项人工 case](../tests/cases/register-cache-lifecycle.md)，人工 case 均为 SKIPPED。该历史批次仅新增勾选 REG-106，批次结束时 **已完成／未完成 14/57**；当时 REG-109 的完整共享 owner、REG-210 的完整实际路由、终端视觉、其他系统及 writer 类别、工具交付和最终 Release 仍未完成。安装版本仍为 0.9.3，本轮没有安装、发布或上板。

2026-10-05 基础框架自检批次：补齐目录实际读取的 4 MiB 边界，含增长输入、UTF-8、组／寄存器数量、间接循环和全部 reader 缺参的验证。宽窗口固定四列，长客户名称和 128 位值不挤掉 Size／Access；窄窗口及字段说明使用自身位宽／访问覆盖。增加同 owner／会话／核／帧的前次有效值比较，字段独立高亮；帮助补齐全部枚举、bit segments、父／字段说明及完整原始值。实际 worker／MI 新增 128 位父值和多层别名的一次读取证明，并核对逐项错误、时间及兼容 Snapshot 输出。

完整功能入口 unit suite 为 **328 单元＋113 集成通过，2 ignored**，严格 Clippy 通过；F24 的 53 个模式均有证据，其余 22 个功能套件未选择。报告及逐项要求见 [框架自检](register-framework-audit.md)，对应 [环境 case](../tests/cases/register-framework.md) 均未执行。自检后勾选 REG-101/103/104/105、REG-201/202/203/204/207/209，合计 **已完成／未完成 13/58**。REG-208 实际终端视觉、配置／生命周期／完整共享 owner／交付矩阵、EL1/Guest VFP、系统及银行 writer、完整目标类别与最终安装 Release 仍待完成。本轮在 `codex/register-debugging` 提交并推送；已安装版本仍 0.9.3，未上板或发布。

2026-10-05 VFP 主机写入批次：S/D/Q 的独立 writer 元数据、显式 `vfp_write_command`、服务锁内 preview/apply/cancel 和物理 owner 绑定已接入。Preview 不写数据；Apply 重新验证实际帧／线程、权限、MVFR、协议和路由，单次发送原始位，以发送时的新鲜 pair 保留邻接位；Q 明确非原子。回执完整宽度、实际特性、expected 原始位及结果必须一致，伪造或结果未知使共享服务 FAULT，不回放或猜测恢复。写后使旧样本／S/D/Q 别名、其他草稿及关联视图失效；上下文变化时 verified 降为 accepted，已有 mismatch 保留。R52/R52+ 目录均声明 80 个原始视图，但实际 R52+ 身份仍不授权；M4 与 FP 控制条目不开放该 writer。同步目录生成器，严格拒绝 S alias 指向普通 GDB reader／错误 D／偏移或非普通写语义。

完整 `cargo test --locked` 为 **319 单元＋112 集成通过，2 ignored**，严格 Clippy 通过；本批 VFP 写入有 9 项 worker/Tcl/TCP 集成，含 Scope All 不广播、权限／实际帧变化、新鲜邻接位、128 位 BE 输入、取消／重复／别名令牌、伪造回执与不确定结果隔离。键鼠及 45×12／80×24 渲染测试核对 32/64/128 位对象和独立配置门禁。实际 DebugTUI 主机延后驱动的 S/D/Q 正常流程各 5 阶段通过，unknown/mismatch 负向流程确认不恢复／重试，并保留 unknown 后共享服务的退出错误。默认 4 skipped；八类 [写入硬件 case](../tests/cases/register-vfp-writes.md) 已更新，均未上板。完整日志为 `artifacts/vfp-host-write-cargo-test.log`、`artifacts/vfp-host-write-clippy.log`，主机报告位于 `artifacts/register-vfp-host-write-*/`。F26 证据映射已加入本批测试；没有据局部测试把完整 feature 勾选。

VFP 主机写入批次结束时，按 71 项 TODO 的完整验收口径，**已完成/未完成为 3/68**；最新计数见上述框架自检批次。合法 EL1/Guest/User VFP、FPSCR/FPEXC 状态 writer、系统／银行写入、64 位 MMIO、完整 GIC/Debug/STM 与最终工具集／版本化安装交付仍待完成。用户要求已改为每轮在 `codex/register-debugging` 提交并推送，前序提交已推送到 `40f260b`；完成全部任务后再发布 Release。已安装工具和项目版本仍是 0.9.3。

2026-10-05 VFP 写入后端批次：独立 `aarch64 vfp_write` 协议支持实际 R52 Hyp 的 S/D/Q 原始位写入，重新确认物理权限和能力；新鲜 pair 保留 S/D 相邻位，Q 为两个非原子 D 写入。生产事务物理恢复／回读 R0/R1、完整 pair、DSPSR/HCPTR/FPEXC/MVFR，未知结果停止，不重试或猜测 rollback；有待写 pair／状态 cache 时注入前拒绝，发送后使后端 D alias 有效标记失效。生产 C 测试覆盖 80 个视图、144 个故障点；独立 GNU Arm 编码验证、5 项 TCP 延后驱动测试通过；Windows/Linux 新目录构建后重新编译最终修复，实际命令检查与 Windows 9 项包校验通过。既有 VFP Rust 回归 11 项集成、5 项单元通过；本批未改 Rust writer 接口。证据见 `artifacts/openocd-vfp-write-linux/`、`.dev/openocd-windows-vfp-write/windows-tests/` 与 `artifacts/vfp-write-driver-native/`。

该后端批次新增 [八类 VFP 写入延后 case](../tests/cases/register-vfp-writes.md)、独立底层硬件驱动与 core/peer 模板，默认 4 skipped，未执行上板。当时 DebugTUI 草稿/编辑/服务锁接入尚未完成；最新状态以上方主机批次为准。底层后端不是整个浮点写 feature 的完成证明。

2026-10-05 位域批次：Watch／Locals typed writer 接入实际 DWARF 1–64 位布局、声明类型／父大小、目标与 RAM 字节序一致性、signed 符号扩展和完整父 RAM／GDB 可能扩大访问范围校验。Preview 不写；Apply 在服务锁内重新解析并读取新鲜父字节，只发送一次类型赋值，再核对字段和全部邻接位。字段匹配但邻接变化仍为 mismatch，回读失败为 accepted，发送后错误为 unknown，不重试／回滚。真实本机 GCC/GDB 六阶段含 unsigned 5/7/14/64 位、signed 6/63 位边界、packed 跨字节、越界／取消／限定符；严格模型覆盖大小端、新鲜邻接、布局／字节序／范围变化、RUNNING、回读／未知／清理和双核 Scope All peer 保持。完整 Cargo 回归 315 单元、103 集成通过，2 ignored；严格 Clippy 通过，修正一项条件合并 lint 后布局三单元复核通过。`artifacts/functional-1791160367322-c3976d3d/report.json` 的 unit／variable-write／variable-reference／variable-bitfield 四 suites 通过，实际 native 阶段分别 9／8／6，F26 映射齐全，其余 19 suites 未选。GNU Arm 11.4 以 R52 参数编译的大小端 ELF32 在配套无 Python ARM GDB 中核对真实字段偏移及原始节字节，两项通过，记录在 `artifacts/variable-bitfield-arm-1791159917132-9988b3ac/report.json`；这是离线布局证明，不执行目标赋值。延后驱动默认 4 skipped，实际二进制＋软件模型五阶段通过，只有 verified 才显式恢复。详见 [位域写入](variable-bitfield-writes.md) 与 [BF-H 案例](../tests/cases/variable-bitfields.md)。long double、完整继承／歧义布局映射、配套 ARM GDB 精确 NaN、其他读写类别、工具集和完整交付仍待完成。未上板、安装 DebugTUI 工具集、推送或发布。

2026-10-05 引用与宽整数批次：typed writer 接入可赋值 C++ lvalue／rvalue reference，保留 referent 限定符、实际 RAM 地址和 owner；使用 `__typeof__(*(&(expression)))` 构造值，不修改 reference 绑定槽。128 位整数改用实际 DWARF 对象类型构造高低字，Preview／Apply 均检查完整位模式，截断或能力变化不发送赋值。自检发现实际 GDB 允许取得位域地址，`sizeof` 也返回容器大小；新增父类型直接字段声明校验，在 Preview／Apply 拒绝位域、继承或无法确认的成员，模板参数不再影响外层指针／限定符判断。实际 C++/GDB 八阶段验证全局／局部／成员／聚合引用、指针和精确 NaN 引用、signed／unsigned 128 位边界、位域拒绝、邻接与函数计数；严格 MI 模型覆盖地址／类型／权限变化、多核 Scope All peer、截断／RUNNING／清理／回读／未知结果。引用与 128 位延后驱动在实际二进制＋模型各 5 阶段通过；默认 4 skipped。完整 Cargo 回归 312 单元、97 集成通过，2 ignored；严格 Clippy 通过。`artifacts/functional-1791157039720-3cb3697e/report.json` 的 unit／variable-write／variable-reference 三 suites 通过，实际本机普通／特殊变量 9 阶段和 C++ 引用／宽整数／位域拒绝 8 阶段通过，F26 映射齐全，其他 19 suites 未选。R52 C++ fixture 已用 GNU Arm 11.4 编译为 ARM ELF32 EABI v5，配套无 Python ARM GDB 的实际引用类型／sizeof／UInt32 常量检查通过，记录在 `artifacts/variable-reference-arm-types-1791155278733-04afc9c9/report.json`；该 Arm 编译器无 `__int128`，不外推为 R52 标量或 Q 向量 writer 支持。详见 [引用写入说明](variable-reference-writes.md)、[引用延后案例](../tests/cases/variable-references-wide.md) 与 [位域案例](../tests/cases/variable-bitfields.md)。位域 writer 仍需 DWARF 位宽／容器范围／新鲜邻接位事务验证，继承成员映射也未适配；完整 TODO、工具集和发布仍待完成。未上板、替换 DebugTUI 工具集、推送或发布。

2026-10-05 特殊变量浮点批次：Watch／Locals typed writer 接入正负 Infinity 和精确 quiet／signaling NaN payload。Preview／Apply 核对 GDB 常量原始位；不匹配时使用按目标字节序构造的主机 Python buffer，重新核对后才发送一次类型赋值。常量检查之后再次验证停止上下文；取消、能力缺失和位模式不符不发送写入，发送后错误不重试，清理错误保留实际结果。完整 Cargo 回归 309 单元、89 集成通过，2 ignored；严格 Clippy 通过。`artifacts/functional-1791153172094-f4f1823d/report.json` 的 unit／variable-write suites 通过，F26 映射齐全，其他 19 suites 未选；本机实际 GCC/GDB 的 9 阶段含 16 次特殊值写入及独立位读取、邻接与函数计数验证。WRITE-H01 float／double 驱动在实际二进制＋大小端 MI 模型分别 6 阶段通过，默认 5 skipped，R52 固件对象编译通过。配套 ARM GDB 无 Python：加载真实 R52 EABI 对象后 8 个 Infinity 常量通过，精确 NaN buffer 一项 skipped；本机 Python GDB 的大小端 24 常量通过。详见 [写入路径和实际限制](variable-special-float-writes.md) 与 [延后案例](../tests/cases/variable-special-floats.md)。变量引用、位域、128 位实际后端和配套工具集精确 NaN 支持仍待完成；未上板、安装、推送或发布。

2026-10-05 状态与取消批次：补齐 Total／Shown／当前 Valid、可读定义和八类状态计数；字段／折叠不重复计数，完整错误／来源／时间可键鼠滚动访问。当前单项硬件缺失也参与 Target 筛选，All 中不自动试读。读取／Probe／MPU／selector 采用独立请求取消标记，原子事务仍完整恢复，整批新结果丢弃，旧样本保留；Scope All 只取消当前 worker，恢复未知仍 FAULT。完整回归 308 单元、83 集成通过，2 ignored；严格 Clippy 通过。`artifacts/functional-1791150195450-fbd63962/report.json` 的 unit suite 通过，F24 的 42 个模式均有证据，其余 20 suites skipped。REG-205/206/211 的软件要求已核对；实际 PowerShell／VS Code 视觉验收和 [人工环境 case](../tests/cases/register-status-cancel.md) 未执行。完整 DDI 0568 的 HCPTR、调试态 FP 陷阱和 DCPS2 UNKNOWN 状态核对见 [方案限制](register-read-status-and-cancel.md)；EL1／Guest VFP 仍未完成。未上板、安装、推送或发布。

2026-10-05 VFP 批次：接入显式 `vfp_command` 和独立协议，Hyp 物理 DSPSR／MIDR／HCPTR 验证后读 VMRS 与 D pair；未改模式或 FPU 控制。依据 R52 TRM §§16.5–16.6 和完整 DDI 0568 D1.3，SP-only D16 与 DP/NEON D32 分开适配，Single／Double／Quad 的 S/D/Q 共用一次物理 pair，保留全部 128 位。十五项 Probe 新增 FPSID、MVFR0/1/2 和实际 FPEXC.EN；未使能、未实现、未知特性和访问受限分别记录。生产 C 事务覆盖 63 个故障点，物理 R0/R1 恢复回读，以及完整 DSPSR／HCPTR／FPEXC 变化拒绝。完整 Cargo 回归 299 单元、76 集成通过，2 ignored；严格 Clippy 通过，F24 的 34 个模式有实际通过证据。REG-H03 默认 4 skipped；实际二进制双核软件驱动的 D32、D16、未使能、TCP10 受限四种流程各 5 阶段通过，独立 GNU Arm 采样钩子编译通过。自检修复 REG-H02/H03 对汇编无类型参考数组的 GDB 读取，显式指针表达式通过真实 GCC/GDB 高位原始字验证。Windows/Linux 新目录后端构建、原生协议／参数／状态检查和 Windows 9 项包检查通过，源码 ZIP 的四个固定提交解压／对象／独立 clone 回读通过；记录在 `artifacts/openocd-adapter-windows-vfp/summary.json` 和 `artifacts/functional-1791147270982-4af639a5/report.json`。合法 EL1/Guest/User VFP 读取、R52+ 实际身份、完整 TODO 和最终安装发布仍未完成；未执行上板，未推送或发布。

2026-10-05 银行批次：接入专用 banked reader 和独立后端协议，内置目录的 23 个银行不再回退旧 mode-switch DPM。依据本地完整 DDI 0568A.c 架构补充，当前银行用普通 MOV／MRS，其他银行只用合法 banked MRS；读取物理 DSPSR 保存的完整停止 CPSR／MIDR、保存恢复回读 R0，并复核全部 DSPSR 位，包括 T／IT。User 和非 Hyp 的 Hyp 银行安全拒绝，未知 R52+ 身份不猜测；实际帧／线程变化丢弃值，结果不确定停用共享通道。Linux 一键脚本和 Windows 脚本各完成全新目录构建，原生生产事务／协议／参数／状态检查通过；C 测试覆盖 20 个故障点。Cargo 回归为 294 单元、66 集成、2 ignored，严格 Clippy 通过。REG-H02 默认 4 skipped；实际二进制在双核软件模型的 Hyp 基线／User 拒绝流程各 5 阶段通过，独立采样钩子 8 个模式汇编通过。未执行上板测试；VFP、完整能力／写入／界面验收及最终安装发布仍待完成。

2026-10-05 Windows 后端批次：固定源码／依赖的全新目录 MinGW-w64 构建通过，包含 J-Link、CMSIS-DAP HID／USB、ST-Link 和 FTDI。Windows 原生生产事务、协议、参数／状态拒绝、静态 DLL 导入、两份 F429 配置和两种 CMSIS-DAP 后端的离线检查通过；9 项包完整性／负向检查通过。源码 ZIP 解压后四份固定提交的 Git 对象检查和独立缓存 clone 回读通过，保留对应源码、配方和许可；修复 ZIP 遗漏 Git 空 refs 目录的问题。动态系统组件与 USB 驱动由宿主提供，离线检查不证明探针通信。当前工具集和安装未替换，未连接物理探针或上板。本批未修改 Rust 运行时，最近 Cargo 完整回归仍为 292 单元、58 集成、2 ignored。最终工具集／profile 整合、安装升级及完整任务验收仍待完成，未推送或发布。

2026-10-05 MRRC／ISB 批次：292 项单元、58 项集成测试通过，2 项 ignored；严格 Clippy 通过。新增显式 64 位 MRRC 通道、真实 ISB 同步和协议门禁，Scope All 仅访问选中核。固定源码 OpenOCD 补丁在 WSL 完整编译 Linux 候选，并通过真实命令的离线拒绝检查；生产事务头文件在 Windows／Linux 编译验证 19 个故障点及临时寄存器恢复。Timer 钩子用 R52 编译参数编译通过；延后驱动默认 5 skipped，实际 DebugTUI 二进制在双核软件模型六阶段通过，未执行上板测试。Windows 后端、完整一键新目录构建和生产探针打包仍待完成。配置与限制见 [适配说明](../tools/openocd-adapter/README.md)。本批先保留本地，完整任务完成后统一推送与发布。

2026-10-05 MPU／MAIR 批次：288 项单元测试、53 项集成测试通过，另 2 项 ignored；严格 Clippy 通过。新增当前核心 EL1／EL2 全部实现区域的直接读取、SCTLR／HSCTLR 全局状态、HPRENR／HCR 和 MAIR 内存属性解码；总览支持键鼠、滚动与显式 Probe／Read。实际二进制延后用例在双核软件夹具完成 5 阶段，所有选择器和控制状态未变；未执行上板测试。完整任务仍在开发，自检完成后统一推送非主分支并发布。

2026-10-04 显示与偏好批次：282 项单元测试、44 项集成测试通过，另 2 项 ignored；严格 Clippy 通过。新增精确整数／浮点／向量显示与芯片、核心和目录隔离的视图持久化。实际二进制的两个独立客户端并发保存用例六阶段通过，Windows 配置保存使用独占句柄和原子替换；没有启动 GDB 或访问板卡。显示格式不证明浮点后端可读。按最新交付要求，本批先保留本地，完整任务自检后统一推送非主分支并发布。

2026-10-04 选择器批次的完整测试通过 273 项单元测试、43 项集成测试，另 2 项既有环境测试 ignored；严格 Clippy 通过。实际二进制的延后用例在双核 Scope All 软件夹具上检查当前核及 peer，5 阶段通过，客户工程未变。未执行上板测试或发布新 Release。

## 已实现并经过本地验证

- `registers_mpu` 默认只解释当前缓存；显式 `read=true` 才执行直接索引读取。整组目录先验证，随后持有完整服务租约；实际模式、MIDR 和数量核对先于可选／索引访问，末尾复核线程／帧及模式。数量为零的 EL2 不访问区域或 MAIR；普通项目错误隔离，恢复未知则隔离服务、不重试，错误上下文废弃 Probe 和旧值有效状态。Scope All 不广播。
- MPU 总览保留每个区域的原始证据、包含末地址的限址、使能、权限、XN、Normal SH 和 MAIR 解释。MAIR 两半独立，未知／过期数据不参与推导；原始值及编码保留，不能从总览推断地址经过 MPU 两级组合后的实际权限。延后 REG-H04 驱动与固件钩子已准备，默认 skipped；软件夹具明确不作为上板能力证明。

- R52 的显式 `registers_select`／Read bank：根据当前 Probe 的实际数量访问 EL1／EL2 MPU 成对区域或 32 位 PMU 事件类型／计数器。固定事务使用服务租约、当前物理 frame 0 及线程核对，单次 TCL 内保存／选择／读取／恢复选择器和 target，并分别回读确认。所有索引和原恢复值都校验；PMSELR=31 仅允许原样恢复。恢复失败与结果未知停用共享通道且不重试。普通失败只使对应成对样本不可用，保留旧值时间；实际身份／数量变化清除能力 Probe。UI 显式动作单个在途请求并丢弃迟到错误；Scope All 不广播。
- 普通 Read 保留直接索引优先路径，目录新增 R52 的 4 个直接 PMEVCNTRn／PMEVTYPERn。选择器 MCR 独立显式配置并匹配 MRC 家族；同步仅在实际控制寄存器 CP15BEN 已设置时使用 CP15ISB，不使能该位或修改 MPU／PMU 配置。MPU 成对返回基址、含末地址的限址、使能及权限；完整区域和 MAIR 总览使用单独的直接读取动作。
- `tests/selector_access.rs` 执行真实 Tcl 8.6 控制流与实际 worker／TCP 事务；软件模型只提供寄存器行为。覆盖 EL1 index 23、EL2 index 19、PMU index 3、发送后错误、同步失败、原选择器异常、恢复写／读回／屏障及 target 恢复失败、通道隔离、失败缓存和双核当前 owner。延后脚本默认 4 skipped；实际二进制＋MI／Tcl 软件夹具的 5 阶段通过，报告 `board_tests_executed=false`，没有执行上板测试。

- 内置 cortex-m4、cortex-r52、cortex-r52+ 目录及可覆盖的用户目录。R52+ 目录目前只包含与 R52 共用的描述，尚未据 R52+ 专用手册确认差异。
- 目录版本、大小、组引用及循环、重复 ID、读取编码、位宽、字段和别名校验。原始值使用精确的 32/64/128 位十六进制字符串，支持非连续 CPSR IT 字段和枚举。
- 项目显式目录优先于 CPU 预设；用户同名 CPU 目录优先于程序内置目录。芯片的用户 CPU 关联优先于已知芯片的内置关联；项目显式选择可以覆盖关联。CPU 与目录都未指定时保留原有动态 GDB 列表。
- Setup 的 CPU 选择、目录浏览、来源显示、取消及保存；路径相对声明它的项目或工具配置解析。升级初始化用户 `profiles/registers/` 目录，不复制会长期遮蔽新内置版本的默认文件，也不改写客户文件。
- System Regs 树的 Core/SIMD/System 分组、字段展开、搜索、筛选、目标／全部定义视图、宽／窄布局以及描述。分组和未读取项目不触发目录扫描式读取；可见条目按批请求，最多 128 项、单个在途请求。
- `registers_list` 和 `registers_read` JSON 接口。逐项隔离 GDB 读取失败，保留名称列表中的空位及原索引；配置目录时停止沿用旧自动寄存器批量刷新。
- `Snapshot.registers` 保留原有结构，投影已经按需获取的 Core GDB 值；新增 `register_samples` 提供精确值、状态、原始原因、归属、来源和时间。继续运行、换停止点、会话或相关栈帧后旧结果失效。
- core/cluster/chip 显式拓扑；未知 cluster 不根据核心编号猜测。实现条件为 Yes/No/Unknown，配置事实的来源标为 configuration，不把目录存在或访问失败当作硬件已实现／未实现。
- 显式 `registers_probe` 和 System Regs 的 Probe caps：固定十五项 CPSR／身份／MPU／PMU／GIC／VFP 能力样本绑定当前物理核心、线程、frame 0 和停止代次，前后检查实际 GDB 线程／帧。当前只识别 Arm D13 Cortex-R52；身份未适配时停止可选探测。非 Hyp 不读取 HMPUIR、物理 ICC 或 ICH；未知 GIC／PMU 不推测读取。原始值、错误、名称可见性及观测事实来源保留于 JSON 和 Log。CPACR 不推断 FPU 存在或启用。
- 观测事实仅在相同上下文覆盖配置声明；下一停止点、换帧／会话或写入后失效，客户事实配置不变。多核 Snapshot 单列 worker `register_generation`，界面请求不再误用协调器刷新版本；Scope All 不广播探测。R52 EL1 目录补足 24 区域，新增只读 ICH_VTR；物理 ICC 与虚拟 ICH 的能力分别解码。
- 32 位 CP15 显式后端命令；内置银行改用经独立协议门禁的专用 `aarch64 banked`，当前／非当前银行均不改变模式，缺少配置或协议时返回 Reader unsupported。单个 TCL 请求内保存、选择、检查实际核心暂停状态并恢复 target。自定义旧 `backend` reader 仍是通用通道，不具有该专用银行保证。MMIO 使用显式组件和既有内存通道。存在读取副作用的项目只允许显式手动读，WO 和明确未实现的项目不读取。
- OpenOCD 服务级访问锁覆盖各核心的 GDB 请求、TCL 访问及 Live Watch。目标恢复失败或可能改变状态的事务结果未知时，共享服务停止后续访问；显式重连才解除。普通目标限定的只读总线查询断线仍可重连。
- Setup 可编辑内存通道的 ID、名称、TCL endpoint、实际 target、运行时访问声明和核心限制；取消丢弃草稿，保存项目覆盖不改写继承的工具配置。Watch 和 Peripherals 均有可见的 Memory access 入口，显示实际路由及配置来源。
- 内存监视策略按芯片、核心和条目保存；重连、切换停止点或栈帧后重新解析地址，丢弃旧绑定和迟到响应。总线读取返回精确原始值及路由信息；64 位普通内存读取明确标记为两次 32 位总线访问，不宣称原子性。
- Memory 面板也有可见的范围／通道设置和 Read 按钮，每次读取 1–4096 字节；设置按芯片和核心保存。暂停时打开面板按当前停止点读取一次，运行时仅手动使用明确声明允许的通道。新 `memory_dump` 接口返回完整连续的字节样本和实际路由；视图检查会话、核心、停止点、栈帧、范围和通道后才接纳结果。
- 写入规划模型保留 32/64/128 位精度、符号、浮点原始位和显式字节序；字段支持非连续位段、父权限／写语义／约束继承及覆盖、读写枚举区分。SVD 不再丢弃枚举、`modifiedWriteValues` 和 `writeConstraint`；缺少 CPU 字节序保持 unknown。规划器为无关 W1C/W0C/置位/翻转位选择“不动作”值，普通邻接 RW 位使用事务内的新鲜值；RO 写效果、保留位、一次写闩锁和不安全读取缺少证据时拒绝。特殊语义已通过实际 worker 的 TCL 软件夹具验证，不作为外设实板能力证明。
- CPU 目录增加与 reader 独立的 writer／write 元数据。内置 r0–r12、SP、LR、PC 使用 GDB 整数 MI writer；暂停的物理 frame 0 才可编辑，预览及应用均查询当前线程状态和 `may-write-registers`，预览还查询实际 MI 命令是否存在及稀疏寄存器索引。GDB 14 的该 writer 使用 LONGEST，明确禁止用它写 128 位向量，不能由数值规划器支持 128 位推断实际 writer 支持。
- `write_preview`／`write_apply`／`write_cancel` 使用服务端草稿，限制数量及有效时间，绑定会话、停止代次、核心、线程、帧和 ELF。编辑与切核／帧／Console／重连使旧草稿失效；Core writer 的 Scope All 只写当前物理核，共享区域按 owner 执行一次。服务访问锁允许同一 worker 的嵌套请求，完整覆盖新鲜读取、写入和验证；超时、断连及发送后错误不重试或回写旧值。结果区分 not_sent、accepted、verified、mismatch 和 unknown，保留原始错误与掩码／路由，写后使重叠视图失效。
- System Regs 的 Edit value 按钮、`e` 键和 `:edit-value` 打开共同编辑流程。Preview、Apply、Cancel 支持键鼠及窄窗口；修改输入废弃预览，错误保留输入，取消迟到预览会清理服务端草稿。发送后关闭不会宣称撤回；无 writer 的系统、银行、浮点和状态对象显示不可写原因。退出时 Watch／断点未变则不重写原工程，避免只读或取消用例改变原始文件。

## 当前能力边界

RAM 与 MMIO 的区域策略使用独立 `[writes]` 配置：必须声明地址区间（end 不包含）、区域种类、通道、owner scope、访问宽度及布局字节序。RAM 字节访问另需 `byte_writable=true`；未知地址、Flash 和从 RAM 入口尝试 MMIO 均在发送前拒绝。GDB 使用 `-data-write-memory-bytes`，不传重复填充 count；TCL 字节写入明确指定 target。读通道可用或允许运行中读取不会授予写权限。

SVD 外设 writer 只发送一个对齐的 8/16/32 位 `target write_memory`；不把拆开的两个 32 位访问声明为 64 位 MMIO writer。字段普通 RW 邻接位在完整服务租约内新鲜读取；W1C/W0C 使用中性值；未知保留位与 RO 写效果必须提供手册依据的 override。查询实际 target `cget -endian` 后在布局和总线整数之间转换，不猜测 AP 的字节序。WO 或读副作用对象在无需新鲜读的整字写入后返回 accepted，不隐式回读。

共享 owner 在预览和 Apply 中核验协调器内部核心状态；JSON 请求不能提交可信 peer 状态。TCL 路由还检查声明的物理 CPU halted_targets，AP 自身暂停不替代 CPU 证明。Scope All 不广播写入，共享写入后使其他核心缓存和草稿失效。Memory 编辑器允许修改字面地址、字节数和地址顺序字节输入；RAM/MMIO 绑定当前栈帧但不要求 frame 0。

新增 `scripts/test-memory-write-hardware.cjs` 准备 WRITE-H04 RAM 子集：默认不连接，显式 --run 才执行专用暂停夹具、取消、写入、独立读取、两侧哨兵及成功后的显式恢复。脚本已在实际 DebugTUI 可执行文件与软件 MI 夹具上运行；报告 board_tests_executed=false。仍需补足 MMIO、缓存一致性及其他 WRITE-H 场景。

64 位 CP15 未配置适配器时仍使用具名 GDB 寄存器；显式 `cp15_64_command` 经协议门禁读取真正 MRRC 的固定 16 位十六进制输出。读不到时绝不用任意两次 MRC 拼接。Windows 候选后端的软件构建与离线命令检查已通过；现有安装仍是旧工具集，物理 Timer 条件未验证。专用银行后端的内部事务已验证无模式写入，并在生产事务中物理回读 R0／DSPSR；这不能证明实板成功路径或调试异常没有副作用。未知浮点／MPU／GIC 条件不自动轮询，Hyp VFP 读取和实际 MVFR/FPEXC 已有软件证据；EL1/Guest 合法读取仍未完成。

上述通道经过隔离的 MI/TCL 软件夹具验证，尚未执行上板验证。用户已指定本任务不执行上板测试；交付仍需准备可执行的相应用例并明确记录未执行。

## 完整任务仍需完成的部分

2026-10-06 按逐项软件证据自检：TODO 有 71 项开发条目，当前已完成／未完成为 28/43；逐项范围见框架、配置、Setup、目录交付、条件依据、生命周期、共享归属、读取来源、状态与取消及能力矩阵自检，完整任务尚未完成。下表覆盖全部条目范围，说明已有实现和阻止完整验收的缺口；“有实现”不表示该阶段的全部要求已通过。最新完整 Cargo 和实际 GDB 回归见上方批次记录。其他功能 suite 仍需在完整任务验收时统一运行。F24／F26 的银行、MRRC、VFP 和变量写入测试映射不能替代整份 TODO 验收。

| 条目范围 | 已有实现与证据 | 尚未完成／需补验收 |
|---|---|---|
| REG-001–008 | 显式当前核 Probe、原始 MIDR/数量/GIC 事实、已配置 MRC/银行路径、隔离的软件多核响应；固定 Windows MRRC/ISB 后端与依赖、候选源码/运行包；只读全目录和12类能力矩阵/完整缓存依据，未验证类显式待补；`capabilities.rs`、`tests/capability_access.rs`、`registers/matrix.rs` | 完整运行工具身份与实际安装对应、GDB 目标描述及各类位宽、FPU/Timer 和 R52+ 差异、矩阵的真实芯片证据、最终 tools/profile 整合及安装升级 |
| REG-101–110 | 严格目录、精确原始值/字段/别名、逐项 reader、上下文/owner、四核显式拓扑与独立共享代次／路由缓存、CPU/目录 Setup、内置与用户目录、三态条件；REG-102 的配置层/路径/芯片关联、REG-108 的完整目录预览／取消／保存／配置差异与当前核心身份提示、REG-107 的目录载荷／初始化／隔离安装／客户文件保留、REG-110 的全父链条件／WO／当前与原值依据已核对；`registers.rs`、`tests/register_access.rs`、覆盖矩阵 F24 | 正式升版后重新验证完整产物、真实版本升级与公网安装；各新增类别的身份/条件/别名适配仍需完成 |
| REG-201–211 | 树、字段、列、说明、搜索、逐核偏好、MPU 总览；新增总数/显示/当前有效/分类计数、完整原因弹窗、实际缺失筛选、请求取消及恢复／Scope All 软件证据；REG-201/202/203/204/205/206/207/209/210/211 已核对，实际路由与保留值来源分开记录 | REG-208 的实际 PowerShell/VS Code 宽窄中文/对比度视觉验收仍需补足；全部目标类别仍需完整验收，不以软件缓冲截图代替终端验收 |
| REG-301–308 | R52 目录、32 位 MRC、直接 EL1/EL2 MPU 与 MAIR、保存/恢复选择器、故障隔离与完整服务锁；真实 ISB／MRRC 和专用银行后端的新目录构建与离线验证；REG-H02 驱动与 8 模式钩子；`session/banked.rs` 及银行事务测试；Hyp VFP/FPSCR 与 MVFR/FPEXC、D16/D32 别名、REG-H03 四类软件用例 | 合法 EL1/Guest/User 读取、更多模式延后用例及未知 R52+ 身份仍未完成 |
| REG-401–408 | REG-402/403 的 Timer MRRC 与单项一致性、REG-404 的当前原生 PMU 容量与 32/64 位读取、REG-408 的原生 ICC/ICH 独立容量及 ICV AP 条件/No 完整依据、REG-406的STM只读配置与独立组件Proof；Windows/Linux 七协议候选与离线固件/延后驱动已有软件证据 | 完整低 EL Timer/ICV 权限、完整 Debug/GIC Distributor/Redistributor MMIO、STM 的实际存在性与映射、Bao EL2/Guest 场景及其他延后驱动；上板未执行 |
| REG-501–506 | 软件回归、严格 Clippy、F24–F26 覆盖来源和限制、增量用户手册/开发记录/示例 | 全部延后案例与原功能回归，完整架构/环境/mcal-vsconfig 配套文档，最终升版、产物/profile 一致性、安装升级、非主分支推送与 Release |
| BUS-001–008 | Setup 通道编辑、各面板入口、路由/来源、范围和绑定失效、运行限制/无回退；`src/launch/channels.rs`、`src/ui/monitor.rs`、`src/session/memory.rs`、F25 | 新增系统寄存器 MMIO 模块的完整接入及相应 BUS 延后场景/Issue #1 完整验收 |
| WRITE-001–012 | Core 整数、声明 RAM、8/16/32 位 MMIO、typed Watch/Locals writer；正负 Infinity、精确 NaN payload 的 GDB 常量检查及条件 Python buffer；C++ RAM 引用、实际 DWARF signed/unsigned 128 位和 1–64 位位域／新鲜邻接事务软件后端；Hyp S/D/Q 独立 raw writer、物理 pair／邻接和旧别名失效、实际二进制延后主机流程；草稿/权限/owner/服务锁/结果/取消；SVD 字段及部分特殊语义；F26 | 系统/状态/银行、合法 EL1/Guest/User VFP 和 64 位 MMIO writer；变量 long double／完整继承成员映射，配套 ARM GDB 的精确 NaN 主机接口；一次写、解锁、自清零等实际策略；全部类别的重叠缓存及 WRITE-T/H 和最终版本化交付 |

测试矩阵自检：REG-T01–T11、BUS-T01–T05、WRITE-T01–T12 必须随上述缺口逐项补足，现有绿色软件测试不能覆盖未接入的类别。延后驱动当前覆盖 REG-H01/H14 能力子集、REG-H02 模式银行独立基线和权限拒绝、REG-H03 Hyp VFP 四类独立基线、REG-H04/H06 选择器子集、REG-H04 完整 MPU/MAIR、REG-H05 Timer MRRC 基线/高字驱动、WRITE-H01 typed 变量子集、WRITE-H02 Core、WRITE-H03 Hyp S/D/Q 底层与 DebugTUI 主机、WRITE-H04 RAM 子集；其余 REG-H/BUS-H/WRITE-H 的可执行用例仍需准备。上板执行按本任务要求不做，交付仍须写好所有相应 case 并记录未执行；不能把 skipped 计为 passed。

完整运行工具版本／哈希与目标描述、可选类别／R52+ 及板级身份能力矩阵（基础显式能力采样十五项，mmio_probe增加42个请求；完整矩阵已形成，真实类别能力仍待补）；最终工具集与 profile 整合、安装升级；完整 GIC 物理／虚拟、Debug以及STM板级映射/其他型号适配；变量 long double／完整继承成员映射、配套 ARM GDB 精确 NaN 支持、64 位 MMIO writer 和其余写入类别的跨面板编辑；系统／银行／FP 状态 writer、合法 EL1/Guest/User VFP、一次写／解锁／自清零策略；全部延后上板用例、完整文档和发布构建、安装、最终推送及 Release。浮点／向量显示、RAM 引用、实际 128 位整数和 DWARF 位域／邻接事务软件后端已有证据，Hyp VFP 实际读取与原始写入通道、别名和主机链路已有软件证据；EL1/Guest 合法访问及完整 PowerShell／VS Code 终端视觉验收仍待完成。R52 编译器无 128 位标量时保持不适用，不把变量软件后端等同于向量 writer。不得因基础框架或部分 writer 通过测试而把完整 TODO 或 Goal 标为完成。

## 当前 writer 矩阵

本批 RAM／MMIO writer 已通过 250 项单元测试和 19 项集成测试，另 2 项环境测试 ignored；严格 Clippy 通过。默认 RAM 延后脚本报告 4 skipped，实际二进制＋软件夹具运行 5 阶段全部通过，未执行上板测试。最新软件证据覆盖 TCL 4096 字节回读和哨兵、大小端 MMIO、shared owner 及客户端伪造拒绝；不能用这些结果代替真实硬件证明。

| 对象 | 实际 writer 与限制 | 证据 |
|---|---|---|
| r0–r12、SP、LR、PC | 独立声明的 GDB 整数 writer；仅暂停、物理 frame 0、单个 owner；PC/SP 提示关联视图变化 | 真实 worker／MI 管道软件夹具；本地 ARM GDB 查询命令存在；实板未执行 |
| CPSR/xPSR 与其他状态／系统／银行寄存器 | 尚无经适配的 writer；不由 reader 或 RW 标签开放写入 | 目录和负向请求测试 |
| D/S/Q 与 FP 状态 | 独立OpenOCD Hyp raw writer v2与DebugTUI草稿／服务锁／物理owner／键鼠编辑已接入；外部MIDR/EDSCR证明当前Hyp，S/D新鲜pair保留相邻位，Q非原子，写后旧别名失效；FP状态和EL1/Guest/User writer未适配；GDB LONGEST通路不得写128位向量 | 生产C 80视图/166故障点、八项底层TCP驱动、九项主机集成、键鼠／窄终端，实际二进制主机S/D/Q各五阶段及不确定／mismatch停止；Windows/Linux后端和延后WRITE-H03 case；未上板 |
| RAM | 声明地址区域、owner、通道与 byte_writable 后，GDB MI 或 target 限定 TCL 字节写入；最多 4096 字节，Flash 走 Download | 实际 worker 软件夹具、64 位地址、范围哨兵、错误及延后脚本验证；实板未执行 |
| SVD 外设及字段 | 声明 MMIO 区域及 TCL 通道；单个对齐 8/16/32 位访问；GDB MMIO 与 64 位 MMIO writer 未适配 | 真实 TCP TCL 软件夹具；大小端、混合语义、WO、回读与错误覆盖；实板未执行 |
| Watch／Locals 标量与结构体／数组成员 | GDB `-var-assign` 类型赋值；实际类型、可赋值性、地址、线程、帧、owner 在预览／应用重新检查；DWARF 位域实际宽度／声明类型、目标字节序、完整父及潜在扩大范围校验，新鲜父字节与全部邻接位验证；无法确认的成员拒绝；声明 RAM，寄存器驻留只接受物理 frame 0；引用要求实际 referent RAM 地址；特殊 float／double 和 128 位常量逐位核对，精确 NaN buffer 依赖实际 GDB Python；const／volatile／AP 路由无隐式回退 | 本机普通／特殊变量 GCC/GDB 9 阶段，C++ 引用／实际 128 位 8 阶段，位域含 64 位／packed 的 6 阶段通过；R52 EABI C++ 对象与配套 ARM GDB 引用类型／常量及大小端位域／原始节字节离线检查通过，该 Arm 编译器没有 128 位标量；MI 大小端／故障／多核隔离与键鼠测试；实板未执行 |
| 混合 RW/RO/WO、W1C/W0C、枚举／保留位／一次写 | 规划器有软件夹具；缺少硬件规则明确拒绝，仅按已声明元数据开放已适配 MMIO writer，不能宣称实板已验证 | `writes::tests`、SVD 元数据夹具；实板未执行 |

GDB writer 的依据为 [GDB 14 MI 实现](https://gnu.googlesource.com/binutils-gdb/+/refs/heads/gdb-14-branch/gdb/mi/mi-main.c) 和实际工作区 ARM GDB 的 `-info-gdb-mi-command data-write-register-values`／`-gdb-show may-write-registers` 输出。SVD 继承及特殊语义依据为 [CMSIS-SVD register 规范](https://open-cmsis-pack.github.io/svd-spec/main/elem_registers.html)。这些软件依据不证明某块板卡的写权限或调试授权。

## 软件验证记录

2026-10-04 能力探测批次：267 项单元测试、37 项集成测试通过，2 项环境测试 ignored；严格 Clippy 通过。新增实际 MIDR／修订、上下文／owner／位宽负向证据、EL1/EL2 数量字段、虚实 GIC 分离、CPACR 非 FPU 证明、原始状态来源及运行时事实覆盖。真实 worker/MI 管道验证显式单次探测、CPU 未适配时停止可选读取、未知 ID 不探测依赖项、物理线程／帧与 RUNNING 变化时整体丢弃、下次停止无自动探测；双核验证正确 worker 停止代次及 Scope All 下当前 owner。

`scripts/test-register-capabilities-hardware.cjs` 准备 REG-H01/H14 能力子集，默认 4 skipped；实际二进制＋软件 MI 夹具 5 阶段通过，未执行上板测试。检查原项目哈希和声明的稳定控制寄存器不变，保留完整 `capabilities.json`、事件与逐项结果；主机工具身份仅在显式提供路径时记录，未知时不猜测。配套 JSON 是填写格式而非 THA6206/Bao 的已确认能力，C 钩子只供独立测试固件调用。其他 REG-H、目标描述位宽、64 位／银行／浮点及完整工具源码/构建矩阵仍需完成。

2026-10-04 变量写入批次：256 项单元测试、27 项集成测试通过，2 项环境测试 ignored；严格 Clippy 通过。新增精确类型赋值、根存储先检查、const／volatile／优化掉与不可赋值拒绝、原 GDB 函数调用策略恢复；地址／类型／权限／帧变化拒绝旧草稿，发送后错误／超时／断连仅一次赋值并准确分类，回读失败保留 accepted，清理失败要求重连。Locals 支持成员树、格式、键鼠展开及跨帧编辑，树收起不访问目标。真实本机 GDB 的 8 阶段报告位于 `artifacts/variable-write-native-1791113030953-3eec4e55/`，原工程 SHA256 保持一致。

新增 `scripts/test-variable-write-hardware.cjs` 和 `tests/fixtures/variable-write-board.example.json`：默认 4 skipped；指定独立暂停夹具、核心、栈帧、类型路径、布局及邻接符号后，执行取消、单次赋值、独立地址／RAM 回读、哨兵和成功后的显式恢复。真实 DebugTUI 二进制与 MI 软件夹具已通过 5 阶段，报告 `board_tests_executed=false`。需按实际工程分别准备 core0/core1、双核共享 owner、调用者帧及各成员用例；不将软件夹具记作上板验收。

2026-10-04 Core 写入批次：247 项单元测试、10 项本地集成测试通过，2 项 ignored；严格 Clippy 通过。新增精度／特殊写规划、SVD 元数据、独立 writer、真实 UI 键鼠与窄布局、失败保留草稿、MI 单次写／取消／权限及线程变化／上下文失效／准确结果状态／无重试。实板 Core 用例默认生成 skipped；全流程通过本地 MI 夹具，报告 `board_tests_executed=false`，未执行板卡测试。其他写类别和完整 TODO 仍未验收，未升版或发布。

2026-10-04：`cargo test --locked` 通过 213 项单元测试和 2 项本地集成测试，2 项依赖外部环境的既有测试保持 ignored；`cargo clippy --locked --all-targets -- -D warnings` 通过。集成夹具确认连接及停止时无额外寄存器读取、稀疏名称索引正确、单项失败不影响后续 64 位值，以及旧值在继续运行后标为 stale。服务测试覆盖并发串行化、独立服务、共享故障隔离和恢复。

2026-10-04 后续回归：220 项单元测试和 3 项本地集成测试通过，2 项既有测试 ignored；严格 Clippy 通过。新增检查覆盖 Setup 通道草稿取消／保存、核心限制和窄窗口操作、换芯片／会话／栈帧后旧监视绑定失效、总线 64 位高位精确输出，以及寄存器读取途中收到异步 RUNNING 时停止批次并丢弃新值。

2026-10-04 Memory 面板回归：230 项单元测试和 5 项本地集成测试通过，2 项既有测试 ignored；严格 Clippy 通过。新增 TCP 夹具验证明确核心路由、64 位地址、地址顺序及拒绝无效范围／上下文；MI 管道夹具验证多块连续内存响应、读取中 RUNNING 通知，以及 Scope All 下仅读取当前核并拒绝另一核的旧上下文。界面测试覆盖窄窗口、草稿取消、逐芯片／核心设置、单个在途读取、各类迟到响应失效和失败不重试／不回退。功能覆盖矩阵已关联目录与跨面板通道软件证据。未连接板卡，未升版、安装或发布。
