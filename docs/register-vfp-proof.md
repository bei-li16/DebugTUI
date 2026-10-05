# VFP 当前 Debug 状态与读取来源

2026-10-06。本轮纠正 REG-304 现有 Hyp 读取路径的权限依据，REG-304 仍待完成，EL1/Guest/User 的合法读取尚未适配。总进度保持26完成／45待完成；未执行上板。

前序协议v1依据保存的 DSPSR.M 判断当前Hyp，再执行CPU MIDR和HCPTR读取。DSPSR描述停止前状态，外部调试指令或异常后不独立证明当前执行EL。现协议v2先经选中target的外部Debug AP读取 EDSCR和MIDR，每次外部访问核对 EDPRSR.HALT；当前EL2才读取HCPTR。EL0/EL1在CPU指令前明确受限，绝不借保存的Hyp状态授权。

依据本地R52 TRM §§16.5–16.6及完整DDI0568A.c；DDI0487 M.b H2.4.2.2.2（PDF第14993–14994页）明确允许VMRS和整数寄存器与D之间VMOV，H2.4.8.2禁止依赖直接当前CPSR/PSTATE读取。R52 HCPTR.TCP10限制仍生效，EN=0不自动使能；Q是位存储视图，不执行SIMD运算。[官方R52 TRM](https://documentation-service.arm.com/static/5f905fedf86e16515cdc25e2)。

读取协议为 `debugtui-armv8-vfp-2 external-identity current-el dspsr dlr vmrs pair-readback no-enable stop-on-fault`。成功响应包含 `midr dscr dspsr dlr hcptr mvfr0 mvfr1 fpexc value` 九对固定字段；前八项精确32位，控制值32位，数据pair完整128位。前后复核外部身份／执行状态及完整DSPSR/DLR/HCPTR/FPEXC，保存、恢复并物理回读scratch。任何I/O故障、恢复失败或状态变化立即停止并隔离target，不继续注入、不发布部分数据或猜测回滚。

Rust校验CPU身份、当前EL2/AArch32、ITE、故障位、HCPTR.TCP10及完整raw宽度，旧协议、旧四字段响应和伪造当前状态被拒绝。所有控制与数据的 `provenance.access.vfp` 保存同次五项原始权限依据；pair数据另保留上一轮的容量与128位证据。S/D/Q在同一请求内共享原始访问，不跨核或停止点复用。详情区分别显示当前Debug EL和停止前DSPSR；失败旧值保留原来源，旧JSON缺字段仍缺证据。

共享writer同步升级为v2，接入同一前置读取；每个物理D写入前再次核对EDSCR，前后完整状态一致后才返回receipt。主机验证receipt附带的五项权限依据；旧或矛盾receipt按结果未知处理，不重试、回滚或发布验证成功。Q仍是两次物理写入，不是原子操作。延后Python驱动直接使用专用响应中的证明，不再单独注入MRC并用DSPSR.M判断当前Hyp。

生产C模型覆盖73个读取I/O故障点、166个写入I/O故障点、八种保存模式与当前Hyp分离、低EL零CPU指令拒绝及外部身份／状态／DLR变化。两项新单元测试验证完整协议与序列化；两项新worker测试验证Scope All选中Core1、八种保存模式、旧或伪造响应、失败原值／来源及后续调试控制。已有80视图、D16/D32、特殊浮点和实际EXE驱动继续回归。

延后驱动及八项环境case见 [VFP证明用例](../tests/cases/register-vfp-proof.md)，全部SKIPPED。软件模型不能证明探针通信、真实调试指令执行或恢复；独立固件RAM、客户配置、工具／ELF哈希和物理双核仍需在允许的环境记录。

EDSCR的DTR满/空位可在独立事务间变化；延后驱动保留完整原始值，每次严格校验身份、EL2/AArch32、ITE、故障及TCP10，稳定比较只使用执行状态mask 0x00053f00。八项TCP测试包含传输位变化通过、低EL零写入拒绝及空AssertionError必须记为失败。首轮反例发现空异常文本曾令failed为false，现记录异常类型；失败日志保留，修复后八项通过。

完整Cargo为 **391单元＋165集成通过，2 ignored，共556通过**；F24的 **149/149** 个证据模式匹配，完整运行538354 ms，无超时，其余22外层套件未选择。总报告和日志逐字节镜像位于 `artifacts/functional-1791238088481-8911e18d/`，核对见 `artifacts/vfp-proof-report-mirror.json`；原53份子报告仍在F盘。严格Clippy通过，日志 `artifacts/vfp-proof-clippy.log`。首次多核专项误用单核worker，修正为实际coordinator后独立及完整回归通过，原失败日志保留。

Windows/Linux均从固定源树完整重建，七套生产事务、七协议及实际dummy命令通过；Windows本机DLL、三项离线配置验证通过。源码ZIP独立审计发现缺少TCP驱动的JSON夹具，已加入包和PROVENANCE；旧包明确拒绝，新包在全新目录脱离工作区编译七套模型、运行八项TCP测试与后端命令通过。Windows包校验十一项通过，新增JSON缺失/篡改两项；既有Git refs负向夹具补齐有效JSON后仍独立检查Git目录，原九项未删减。补丁、十一项源码锁及当前候选哈希见[后端源码自检](register-backend-source.md)，独立解压报告为 `artifacts/vfp-proof-source-package-report.json`。源码可构建和软件事务通过不等同于探针/板级支持。

MIDR仅确认处理器型号，不是唯一核心标识；请求target到物理核的关联仍由工程配置及环境验收确认。本轮不增加TODO勾选，总进度 **26完成／45待完成**，目标active。EL1/Guest/User合法读取、FP状态writer、其余TODO、最终全套验收/升版/工具安装/Release仍待完成，八项环境case全部SKIPPED。
