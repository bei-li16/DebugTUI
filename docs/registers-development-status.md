# 寄存器功能实现进度

## 2026-10-05：Setup CPU／目录预览与不匹配提示（REG-108）

增加 CPU、文件及未提交候选的 F1 只读可滚动详情，显示目录名称、架构、来源、说明、访问条件与 capability fact 范围；配置 CPU、Chip 关联与 Observed CPU 分别展示。M4／R52／R52+、用户 preset、Automatic、GDB 与文件引用的取消／保存经隔离子进程矩阵验证，不改写客户 profile、设备及目录，也不改变核心／backend／启动／build／download 设置。修复 CPU picker 的 F2 错转项目浏览、错误文件关闭浏览器，以及文件来源无 CPU preset 时误显示 GDB 列表。当前身份仅在 STOPPED 且 session／stop／core／frame 全部相同、草稿目标／实际访问路由不变时可用；其他核心或未适配 MIDR 保持 Unknown。只读预览与 Registers status 不提交 probe/read。

最终完整 `node scripts/test-functional.cjs --only unit` 为 **352 单元＋124 集成通过，2 ignored**；严格 Clippy、格式及差异检查通过。F24 的 **87 个证据模式全部通过**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791185248280-a74b4bef/report.json`](../artifacts/functional-1791185248280-a74b4bef/report.json)，完整 Cargo 日志为同目录 `unit.log`，Clippy 日志为 `artifacts/register-setup-clippy.log`。本批新增七项单元测试；逐项范围见 [Setup 目录选择自检](register-setup-catalogues.md)，[十二项环境 case](../tests/cases/register-setup-catalogues.md) 均 SKIPPED。完整回归继续核对前序配置、生命周期、共享归属与读取来源，没有替代或缩小其要求。仅新增勾选 REG-108，当前 **已完成／未完成 18/53**。REG-107/110、REG-208 实际终端视觉、R52+ 实际身份及完整可选类别、其余系统与 writer、全部工具交付、最终安装／Release 仍未完成。本轮提交并推送非主分支；没有上板、安装或发布，安装版本仍为 0.9.3。

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

2026-10-05 按逐项软件证据自检：TODO 有 71 项开发条目，当前已完成／未完成为 18/53；逐项范围见框架、配置、Setup、生命周期、共享归属、读取来源及状态与取消自检，完整任务尚未完成。下表覆盖全部条目范围，说明已有实现和阻止完整验收的缺口；“有实现”不表示该阶段的全部要求已通过。最新完整 Cargo 和实际 GDB 回归见上方批次记录。其他功能 suite 仍需在完整任务验收时统一运行。F24／F26 的银行、MRRC、VFP 和变量写入测试映射不能替代整份 TODO 验收。

| 条目范围 | 已有实现与证据 | 尚未完成／需补验收 |
|---|---|---|
| REG-001–008 | 显式当前核 Probe、原始 MIDR/数量/GIC 事实、已配置 MRC/银行路径、隔离的软件多核响应；固定 Windows MRRC/ISB 后端与依赖、候选源码/运行包；`capabilities.rs`、`tests/capability_access.rs` | 完整运行工具身份与实际安装对应、GDB 目标描述及各类位宽、FPU/Timer 和 R52+ 差异、完整能力矩阵、最终 tools/profile 整合及安装升级 |
| REG-101–110 | 严格目录、精确原始值/字段/别名、逐项 reader、上下文/owner、四核显式拓扑与独立共享代次／路由缓存、CPU/目录 Setup、内置与用户目录、三态条件；REG-102 的配置层/路径/芯片关联、REG-108 的完整目录预览／取消／保存／配置差异与当前核心身份提示已核对；`registers.rs`、`tests/register_access.rs`、覆盖矩阵 F24 | 对最新完整交付范围重新验证 EXE/ZIP/npm 安装升级与客户目录保留；各新增类别的身份/条件/别名适配仍需完成 |
| REG-201–211 | 树、字段、列、说明、搜索、逐核偏好、MPU 总览；新增总数/显示/当前有效/分类计数、完整原因弹窗、实际缺失筛选、请求取消及恢复／Scope All 软件证据；REG-201/202/203/204/205/206/207/209/210/211 已核对，实际路由与保留值来源分开记录 | REG-208 的实际 PowerShell/VS Code 宽窄中文/对比度视觉验收仍需补足；全部目标类别仍需完整验收，不以软件缓冲截图代替终端验收 |
| REG-301–308 | R52 目录、32 位 MRC、直接 EL1/EL2 MPU 与 MAIR、保存/恢复选择器、故障隔离与完整服务锁；真实 ISB／MRRC 和专用银行后端的新目录构建与离线验证；REG-H02 驱动与 8 模式钩子；`session/banked.rs` 及银行事务测试；Hyp VFP/FPSCR 与 MVFR/FPEXC、D16/D32 别名、REG-H03 四类软件用例 | 合法 EL1/Guest/User 读取、更多模式延后用例及未知 R52+ 身份仍未完成 |
| REG-401–408 | Timer/PMU/GIC 部分目录、PMU 数量与直接/选择器读取、物理/虚拟 GIC 能力分离和 AP 条件软件夹具；Windows MRRC 候选构建/命令检查 | 完整 Timer 权限/一致性适配，完整 GIC/Debug/STM 类别及显式板级映射，Bao EL2/Guest 场景及全部延后驱动 |
| REG-501–506 | 软件回归、严格 Clippy、F24–F26 覆盖来源和限制、增量用户手册/开发记录/示例 | 全部延后案例与原功能回归，完整架构/环境/mcal-vsconfig 配套文档，最终升版、产物/profile 一致性、安装升级、非主分支推送与 Release |
| BUS-001–008 | Setup 通道编辑、各面板入口、路由/来源、范围和绑定失效、运行限制/无回退；`src/launch/channels.rs`、`src/ui/monitor.rs`、`src/session/memory.rs`、F25 | 新增系统寄存器 MMIO 模块的完整接入及相应 BUS 延后场景/Issue #1 完整验收 |
| WRITE-001–012 | Core 整数、声明 RAM、8/16/32 位 MMIO、typed Watch/Locals writer；正负 Infinity、精确 NaN payload 的 GDB 常量检查及条件 Python buffer；C++ RAM 引用、实际 DWARF signed/unsigned 128 位和 1–64 位位域／新鲜邻接事务软件后端；Hyp S/D/Q 独立 raw writer、物理 pair／邻接和旧别名失效、实际二进制延后主机流程；草稿/权限/owner/服务锁/结果/取消；SVD 字段及部分特殊语义；F26 | 系统/状态/银行、合法 EL1/Guest/User VFP 和 64 位 MMIO writer；变量 long double／完整继承成员映射，配套 ARM GDB 的精确 NaN 主机接口；一次写、解锁、自清零等实际策略；全部类别的重叠缓存及 WRITE-T/H 和最终版本化交付 |

测试矩阵自检：REG-T01–T11、BUS-T01–T05、WRITE-T01–T12 必须随上述缺口逐项补足，现有绿色软件测试不能覆盖未接入的类别。延后驱动当前覆盖 REG-H01/H14 能力子集、REG-H02 模式银行独立基线和权限拒绝、REG-H03 Hyp VFP 四类独立基线、REG-H04/H06 选择器子集、REG-H04 完整 MPU/MAIR、REG-H05 Timer MRRC 基线/高字驱动、WRITE-H01 typed 变量子集、WRITE-H02 Core、WRITE-H03 Hyp S/D/Q 底层与 DebugTUI 主机、WRITE-H04 RAM 子集；其余 REG-H/BUS-H/WRITE-H 的可执行用例仍需准备。上板执行按本任务要求不做，交付仍须写好所有相应 case 并记录未执行；不能把 skipped 计为 passed。

完整运行工具版本／哈希与目标描述、可选类别／R52+ 及板级身份能力矩阵（显式能力采样为十五项，完整类别矩阵仍未完成）；最终工具集与 profile 整合、安装升级；完整 GIC 物理／虚拟、Debug、STM 配置状态目录及板级映射；变量 long double／完整继承成员映射、配套 ARM GDB 精确 NaN 支持、64 位 MMIO writer 和其余写入类别的跨面板编辑；系统／银行／FP 状态 writer、合法 EL1/Guest/User VFP、一次写／解锁／自清零策略；全部延后上板用例、完整文档和发布构建、安装、最终推送及 Release。浮点／向量显示、RAM 引用、实际 128 位整数和 DWARF 位域／邻接事务软件后端已有证据，Hyp VFP 实际读取与原始写入通道、别名和主机链路已有软件证据；EL1/Guest 合法访问及完整 PowerShell／VS Code 终端视觉验收仍待完成。R52 编译器无 128 位标量时保持不适用，不把变量软件后端等同于向量 writer。不得因基础框架或部分 writer 通过测试而把完整 TODO 或 Goal 标为完成。

## 当前 writer 矩阵

本批 RAM／MMIO writer 已通过 250 项单元测试和 19 项集成测试，另 2 项环境测试 ignored；严格 Clippy 通过。默认 RAM 延后脚本报告 4 skipped，实际二进制＋软件夹具运行 5 阶段全部通过，未执行上板测试。最新软件证据覆盖 TCL 4096 字节回读和哨兵、大小端 MMIO、shared owner 及客户端伪造拒绝；不能用这些结果代替真实硬件证明。

| 对象 | 实际 writer 与限制 | 证据 |
|---|---|---|
| r0–r12、SP、LR、PC | 独立声明的 GDB 整数 writer；仅暂停、物理 frame 0、单个 owner；PC/SP 提示关联视图变化 | 真实 worker／MI 管道软件夹具；本地 ARM GDB 查询命令存在；实板未执行 |
| CPSR/xPSR 与其他状态／系统／银行寄存器 | 尚无经适配的 writer；不由 reader 或 RW 标签开放写入 | 目录和负向请求测试 |
| D/S/Q 与 FP 状态 | 独立 OpenOCD Hyp raw writer 与 DebugTUI 草稿／服务锁／物理 owner／键鼠编辑已接入；S/D 新鲜 pair 保留相邻位，Q 非原子，写后旧别名失效；FP 状态和 EL1/Guest/User writer 未适配；GDB LONGEST 通路不得写 128 位向量 | 生产 C 80 视图/144 故障点、5 项底层 TCP 驱动、9 项主机集成、键鼠／窄终端，实际二进制主机 S/D/Q 各 5 阶段及不确定／mismatch 停止；Windows/Linux 后端和延后 WRITE-H03 case；未上板 |
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
