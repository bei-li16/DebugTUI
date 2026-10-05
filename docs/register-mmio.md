# R52 GIC 与外部 Debug MMIO 自检

本批解决固定 component.base 在多核下无法区分 Debug/Redistributor 私有窗口与 Distributor 共享窗口的问题。开发分支新增显式 owner 路线与 R52 目录，复用已有 GDB/AP 内存读取；不改变调试执行状态，不提供 MMIO writer。软件实现及离线固件证据不证明实板可访问，REG-405/BUS-006 全范围仍待验收。

## 手册与目录范围

主要依据本地 [R52 TRM](<G:/Data/GitFiles/ARM/File/Arm® Cortex®-R52 Processor Technical Reference Manual.pdf>)（100026_0104_01_en）§10.2.1、表 10-4/10-35/10-36/12-5，及 [Armv8-R AArch32](<G:/Data/GitFiles/ARM/File/Armv8-R AArch32.pdf>)（DDI0568A.c）G2.1.5 和 H1.3 DCC 伪代码。只使用手册维护的元数据，ADS 提供树形/字段/原始值对照思路，不依赖其资源文件。

新增 **1,598 项**：GICD 1,485、GICR 33、外部 Debug 80。R52 与 R52+ 目录同步；这是地址/访问描述，未声明另一型号的原生后端已适配。M4 目录不变。寄存器 RW 标签与写语义说明不开放编辑；本批所有 MMIO 项均无 writer/write。

| 组件 | 归属与 base 定义 | 覆盖 |
| --- | --- | --- |
| gicd | 显式 cluster；集成 GIC 的 Distributor control base | CTLR/TYPER/IIDR、SPI INTID32–991 的 group/enable/pending/active、priority/config/router、PIDR/CIDR |
| gicr | 显式 core；Redistributor control 页 | CTLR/IIDR/TYPER/WAKER/ID；SGI/PPI 页在 base+0x10000，包含私有状态、八个 priority 和两个 config |
| debug_external | 显式 core；外部 Debug aperture | 状态、raw feature/身份字、八个 breakpoint/watchpoint、两个扩展比较器，以及副作用/WO 条目元数据 |

R52 GICR_CTLR 和 SGI ICFGR0 为 RO；SPI priority 只解码每字节高五位。IROUTER32 地址是 GICD+0x6100，最高 IROUTER991 为 +0x7EF8；不增加 INTID992–1023 或不存在的 GICD 私有 bank0。GICR_TYPER 和 IROUTER 是完整 64 位 raw，AP 实际为两个 32 位字且 atomic=false；GDB 的一条 memory MI 请求也不证明物理原子性。R52 比较器为 32 位，不能按其他架构拼装 64 位 BVR/WVR。

## 配置、归属与请求

配置示例：[tha6-mmio.toml.example](../profiles/tha6-mmio.toml.example)。所有地址均为虚构示例，默认三个 present=0；合并到实际 chip/environment 配置后，依据板级资料替换地址、核心名称、cluster 与通道。channel 为空走当前选定 GDB 连接；非空引用既有 memory_access ID，继续应用 target、endpoint、core filter、状态和服务锁。

component_owners 的层次为组件名 → 完整 owner → base/channel/little_endian。例如 gicr → core:core.2 → 该物理核窗口，gicd → cluster:A → 共享窗口。owner 前缀只能为 core:/cluster:/chip:；路由严格要求三个字段。任一组件声明了 owner map，就不能回退到旧 components.base；缺少条目、未知 cluster 或 owner 不匹配均拒绝。新内置 MMIO reader 显式 require_owner_mapping=true，即使没有 owner map 也不使用静态默认地址。用户旧目录省略该标志仍兼容原静态配置，JSON 默认 false 不增加字段。

地址只由显式 base 与手册 offset 做 checked_add。CFGPERIPHBASE/CBAR、Aff0、ProcessorNumber、core 标签和连续窗口都不用于推导板级归属。Scope All 仍只读选中 owner；共享样本由协调器绑定 owner_generation，单会话绑定本身的 session/stop/frame context，不制造缺失的共享代次。

每项保存实际请求的源、owner、目录 reader、地址、位宽、字节序、GDB endpoint 或 AP target/endpoint/channel/配置来源、响应阶段及主机请求区间。读取失败不会静默切换通道；保留旧值时继续引用原完整 provenance。内置大目录以 OnceLock 缓存解析结果并克隆。详情在打开时按明确来源加载完整预览，滚动只复用已加载文本并按窗口宽度换行；草稿配置或当前身份证据变化即失效并重载，关闭后重新打开也重新读取用户文件。显式加载和启动校验继续报告错误覆盖，不用缓存兜底。

## 能力与副作用边界

三组件各有 present=1 条件。SPI 银行、priority、config 和每个 IROUTER 还要求 gicd.interrupts 足够；比较器要求 breakpoint/watchpoint 数量。Unknown 不自动请求，明确 No 手工也不请求。**这些新事实当前来自配置，普通读取 TYPER/Debug ID 不会自动转成新鲜的物理 capability Probe**。配置不证明地址上的真实组件、供电/认证/总线权限；本批驱动另外核对 fresh 外部 MIDR，但不能据此授予全部 MMIO 容量。

EDPRSR 的 sticky clear、EDPCSRlo 更新 EDCIDSR/EDVIDSR，以及 DBGDTRTX 清 TXfull/可能触发 MA 后续传输均只允许手工。EDPCSRhi 保守使用手工策略，明确尚未确认该上字 aperture 的精确采样语义，不声称与低字为同一快照。外部 DBGDTRRX read 只返回 DTRRX，不清 RXfull；不能与 CPU 内部接收操作混为一谈。EDITR、EDRCR、DBGOSLAR、EDLAR 四项 WO 均不读。本批不解锁、不注入指令、不清错误、不 acknowledge、enable/deactivate IRQ 或改配置。

EDPFR 摘要表 12-5 的 D20/D24 high/low 与详细表 12-38/39 次序矛盾，因此目录按绝对 offset 命名 word0/word1，分别保存原始字。EDDFR D28/D2C 的摘要与详细表一致、EDAA32PFR D60/D64 详细表为 high/low；本批也只保留独立 raw word，不推断跨请求配对、比较器数量或其他能力。相关上板 case 会逐地址对照独立资料。

## 本批软件证据与剩余范围

三个专项单元覆盖严格配置/旧序列化、owner 缺失不回退、两份 R52 目录的独立地址/计数/字段、容量边界、副作用与 WO。三个集成覆盖四个非连续 worker/两 cluster/未知 owner、真实 AP/TCP 的 64 位字序与实际 target、手工策略/错误保留，以及实际 EXE 的二十四项独立固件 RAM 驱动。

驱动软件正向五阶段通过；not_ready、reference_mismatch、identity_mismatch、owner_mismatch 四种负向流程各产生预期失败并正常断开。前两项门禁场景中未就绪与错误 owner 均零 MMIO；错误身份只访问外部 MIDR；参考高字不一致被拒绝。全过程不写寄存器/内存、不 resume，项目不变。单核驱动和四 worker 隔离证据分开记录，不把单核流程声明为完整双核上板。


完整回归发现大目录详情每次滚动重读/解析用户 TOML；确认对应进程仍运行后，仅结束已核对路径和父进程的本轮 unit-test，首次报告 artifacts/functional-1791223962386-7a29317c/report.json 为失败（507915 ms），停止依据 artifacts/register-mmio-slow-preview-abort.json。修复后原完整滚动断言与新开窗 snapshot/重开解析错误用例通过；下一完整报告 artifacts/functional-1791224680279-11dcbf6f/report.json 发现旧 CPU 身份缓存未按 RUNNING/草稿 endpoint 变化清除。已增加草稿/当前身份键失效，保留原身份断言；五项预览首次聚焦记录 artifacts/register-mmio-preview-focused.log，后续聚焦为 artifacts/register-mmio-preview-key-focused.log 与 artifacts/register-mmio-preview-identity-focused.log。这些失败不计为通过。下一完整回归 artifacts/functional-1791225055288-a044a95b/report.json 中单元测试全部通过，但旧配置驱动在四个大目录场景触发 Node 默认 1 MiB stdout 缓冲 ENOBUFS；已把客户端设为与既有分发测试一致的有限 32 MiB，并在断言前保留输出，全部十项配置优先级/不启动 GDB/文件不变断言保留。复跑记录 artifacts/register-mmio-configuration-buffer-focused.log。

首次工作集成编译失败因测试访问 Project 私有 preference_core，已改用公开 Core 配置和真实 coordinator。首轮新驱动错误假定单会话都有 coordinator 的共享代次，已按真实 list/snapshot 结构分别检查；失败保留在 artifacts/register-mmio-worker-focused.log 和 artifacts/register-mmio-driver-focused.log，通过复跑为 artifacts/register-mmio-worker-retry.log、artifacts/register-mmio-unit-final-focused.log、artifacts/register-mmio-driver-retry.log。

独立 C 固件已用 GNU Arm 11.4.0、cortex-r52/marm/ffreestanding/Wall/Wextra/Werror 离线编译。手写地址清单核对二十四项（二十六次外部 32 位字），反汇编验证一次 MIDR MRC、前置 CPSR guard，无 MCR/MCRR/MSR/MRRC 控制或模式 opcode；源/对象/反汇编散列见 artifacts/register-mmio-firmware-report.json。此证据不证明运行时总线地址/权限，也未执行对象代码。

[十二项环境 case](../tests/cases/register-mmio.md)、独立 C/JSON 和默认五阶段 SKIPPED 驱动已准备；默认报告 artifacts/register-mmio-hardware-1791224011438-fe09a965/report.json。现有原生适配器的十一项源锁、补丁、Windows/Linux 候选与对应源码 ZIP 再核对一致，见 artifacts/register-mmio-inherited-package-audit.log；本批不修改该后端。

剩余：新鲜 MMIO 容量/组件身份 Probe、完整 Debug system route、低 EL ICV/Timer/VFP 权限、STM/Bao 与其他类别、BUS 完整逐项验收、writer、真实终端视觉检查、工具整合、全套最终验收和升版/安装/Release。REG-405/BUS-006 未勾选；完整 TODO 保持 **24 完成／47 未完成**，不将本批的一个子集改写为完整目标。

## 最终本轮回归

完整 Cargo **379 单元＋153 集成通过，2 ignored，共 532 通过**；**F24 132/132** 是证据匹配模式数，不能当作用例数。整套 497753 ms，无超时；其余 **22 功能套件未选择**。报告 artifacts/functional-1791225291601-66910f0b/report.json，完整 Cargo 为同目录 unit.log；严格 Clippy 通过，日志 artifacts/register-mmio-clippy-final.log。

另新增一项目录详情开窗快照/重开错误回归，原大目录完整滚动与当前身份清除回归全部保留并通过。实际生产分发、配置优先级/多核、缓存生命周期、来源/状态/取消、原生 GIC/PMU/Timer/MPU/银行/VFP 与既有 writer 的 Cargo 回归一起通过；该结果不替代未选的二十二个外层功能套件、终端视觉或硬件验收。目录重新生成比对、M4 无差异、格式与 git diff --check 通过。最后源包/候选一致性日志 artifacts/register-mmio-inherited-package-final-audit.log。
