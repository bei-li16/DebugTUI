# 寄存器功能实现进度

本文件记录开发分支上的实际实现，配合 [开发 TODO](registers-development-todo.md) 使用。当前仍是未发布的开发版本；下述软件验证不能作为芯片或 OpenOCD 实板能力证明。

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
- 显式 `registers_probe` 和 System Regs 的 Probe caps：固定十项 CPSR／身份／MPU／PMU／GIC 能力样本绑定当前物理核心、线程、frame 0 和停止代次，前后检查实际 GDB 线程／帧。当前只识别 Arm D13 Cortex-R52；身份未适配时停止可选探测。非 Hyp 不读取 HMPUIR、物理 ICC 或 ICH；未知 GIC／PMU 不推测读取。原始值、错误、名称可见性及观测事实来源保留于 JSON 和 Log。CPACR 不推断 FPU 存在或启用。
- 观测事实仅在相同上下文覆盖配置声明；下一停止点、换帧／会话或写入后失效，客户事实配置不变。多核 Snapshot 单列 worker `register_generation`，界面请求不再误用协调器刷新版本；Scope All 不广播探测。R52 EL1 目录补足 24 区域，新增只读 ICH_VTR；物理 ICC 与虚拟 ICH 的能力分别解码。
- 32 位 CP15 显式后端命令和 `get_reg` 银行寄存器适配路径；单个 TCL 请求内保存、选择、检查实际核心暂停状态并恢复 target。MMIO 使用显式组件和既有内存通道。存在读取副作用的项目只允许显式手动读，WO 和明确未实现的项目不读取。
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

64 位 CP15 未配置适配器时仍使用具名 GDB 寄存器；显式 `cp15_64_command` 经协议门禁读取真正 MRRC 的固定 16 位十六进制输出。读不到时绝不用任意两次 MRC 拼接。生产 Windows 后端及物理 Timer 条件尚未验证。32 位 CP15 命令、银行名称和可选寄存器需要与实际 OpenOCD 构建核对。未知浮点／MPU／GIC 条件不自动轮询。客户端未发送切换 CPU 模式或使能 FPU 的命令；银行和浮点后端的内部行为尚需确认，不能据此宣称后端无副作用。

上述通道经过隔离的 MI/TCL 软件夹具验证，尚未执行上板验证。用户已指定本任务不执行上板测试；交付仍需准备可执行的相应用例并明确记录未执行。

## 完整任务仍需完成的部分

2026-10-05 按用户新要求自检：TODO 中有 71 项开发条目，完整任务尚未完成。下表覆盖全部条目范围，说明已有实现和阻止完整验收的缺口；“有实现”不表示该阶段的全部要求已通过。当前完整 Cargo 回归通过 288 项单元、53 项集成测试，2 项 ignored；功能验收仅运行 unit suite，其他 20 个 suite 本轮未运行。F24 的 29 项证据模式都有实际通过记录，不能用该状态替代整份 TODO 验收。

| 条目范围 | 已有实现与证据 | 尚未完成／需补验收 |
|---|---|---|
| REG-001–008 | 显式当前核 Probe、原始 MIDR/数量/GIC 事实、已配置 MRC/银行路径、隔离的软件多核响应；`capabilities.rs`、`tests/capability_access.rs` | 完整运行工具身份与源码构建对应、GDB 目标描述及各类位宽、FPU/Timer 和 R52+ 差异、完整能力矩阵、Windows MRRC 后端及探针/分发构建 |
| REG-101–110 | 严格目录、精确原始值/字段/别名、逐项 reader、上下文/owner、CPU/目录 Setup、内置与用户目录、三态条件；`registers.rs`、`tests/register_access.rs`、覆盖矩阵 F24 | 对最新完整交付范围重新验证 EXE/ZIP/npm 安装升级与客户目录保留；各新增类别的身份/条件/别名适配仍需完成 |
| REG-201–211 | 树、字段、列、说明、搜索、按需批次、目标/定义切换、逐核格式与隔离偏好、MPU 总览；`src/ui/registers.rs` 及显示/MPU 测试 | 目录总数/当前显示/本次有效值及各状态分类计数尚未全部呈现；Reader unsupported 的可见分类、完整读取取消和 PowerShell/VS Code 宽窄中文/对比度视觉验收需补足 |
| REG-301–308 | R52 目录、32 位 MRC、直接 EL1/EL2 MPU 与 MAIR、保存/恢复选择器、故障隔离与完整服务锁；`session/registers.rs`、`session/selectors.rs`、`session/mpu.rs` | 全部实际银行模式与 VFP/FPSCR/使能状态读取、目标限定 Q/D/S 别名能力、Windows 后端中的真实 ISB 适配、全部银行/VFP 延后用例 |
| REG-401–408 | Timer/PMU/GIC 部分目录、PMU 数量与直接/选择器读取、物理/虚拟 GIC 能力分离和 AP 条件软件夹具 | 完整 Timer 权限/一致性适配、Windows MRRC 后端，完整 GIC/Debug/STM 类别及显式板级映射，Bao EL2/Guest 场景及全部延后驱动 |
| REG-501–506 | 软件回归、严格 Clippy、F24–F26 覆盖来源和限制、增量用户手册/开发记录/示例 | 全部延后案例与原功能回归，完整架构/环境/mcal-vsconfig 配套文档，最终升版、产物/profile 一致性、安装升级、非主分支推送与 Release |
| BUS-001–008 | Setup 通道编辑、各面板入口、路由/来源、范围和绑定失效、运行限制/无回退；`src/launch/channels.rs`、`src/ui/monitor.rs`、`src/session/memory.rs`、F25 | 新增系统寄存器 MMIO 模块的完整接入及相应 BUS 延后场景/Issue #1 完整验收 |
| WRITE-001–012 | Core 整数、声明 RAM、8/16/32 位 MMIO、typed Watch/Locals writer；草稿/权限/owner/服务锁/结果/取消；SVD 字段及部分特殊语义；F26 | 系统/状态/银行/浮点和 64 位 MMIO writer；变量位域/引用/NaN/Infinity/128 位后端；一次写、解锁、自清零等实际策略；全部类别的重叠缓存及 WRITE-T/H 和最终版本化交付 |

测试矩阵自检：REG-T01–T11、BUS-T01–T05、WRITE-T01–T12 必须随上述缺口逐项补足，现有绿色软件测试不能覆盖未接入的类别。延后驱动当前覆盖 REG-H01/H14 能力子集、REG-H04/H06 选择器子集、REG-H04 完整 MPU/MAIR、REG-H05 Timer MRRC 基线/高字驱动、WRITE-H01 typed 变量子集、WRITE-H02 Core 和 WRITE-H04 RAM 子集；其余 REG-H/BUS-H/WRITE-H 的可执行用例仍需准备。上板执行按本任务要求不做，交付仍须写好所有相应 case 并记录未执行；不能把 skipped 计为 passed。

完整工具版本／哈希与目标描述、可选类别／R52+ 及板级身份能力矩阵（显式能力采样仍只有十项）；Windows 64 位/ISB 后端、完整新目录构建和生产探针依赖；完整 GIC 物理／虚拟、Debug、STM 配置状态目录及板级映射；变量的位域／引用／特殊浮点及 128 位后端验收、64 位 MMIO writer 和其余写入类别的跨面板编辑；系统／银行／浮点 writer、一次写／解锁／自清零策略；全部延后上板用例、完整文档和发布构建、安装、推送及 Release。浮点／向量显示已完成，但实际 VFP 读取、别名适配和完整 PowerShell／VS Code 终端视觉验收仍待完成。不得因基础框架或部分 writer 通过测试而把完整 TODO 或 Goal 标为完成。

## 当前 writer 矩阵

本批 RAM／MMIO writer 已通过 250 项单元测试和 19 项集成测试，另 2 项环境测试 ignored；严格 Clippy 通过。默认 RAM 延后脚本报告 4 skipped，实际二进制＋软件夹具运行 5 阶段全部通过，未执行上板测试。最新软件证据覆盖 TCL 4096 字节回读和哨兵、大小端 MMIO、shared owner 及客户端伪造拒绝；不能用这些结果代替真实硬件证明。

| 对象 | 实际 writer 与限制 | 证据 |
|---|---|---|
| r0–r12、SP、LR、PC | 独立声明的 GDB 整数 writer；仅暂停、物理 frame 0、单个 owner；PC/SP 提示关联视图变化 | 真实 worker／MI 管道软件夹具；本地 ARM GDB 查询命令存在；实板未执行 |
| CPSR/xPSR 与其他状态／系统／银行寄存器 | 尚无经适配的 writer；不由 reader 或 RW 标签开放写入 | 目录和负向请求测试 |
| D/S/Q 与 FP 状态 | 实际 writer 尚未适配；GDB LONGEST 通路不得写 128 位向量 | GDB 14 源码、宽度拒绝测试 |
| RAM | 声明地址区域、owner、通道与 byte_writable 后，GDB MI 或 target 限定 TCL 字节写入；最多 4096 字节，Flash 走 Download | 实际 worker 软件夹具、64 位地址、范围哨兵、错误及延后脚本验证；实板未执行 |
| SVD 外设及字段 | 声明 MMIO 区域及 TCL 通道；单个对齐 8/16/32 位访问；GDB MMIO 与 64 位 MMIO writer 未适配 | 真实 TCP TCL 软件夹具；大小端、混合语义、WO、回读与错误覆盖；实板未执行 |
| Watch／Locals 标量与结构体／数组成员 | GDB `-var-assign` 类型赋值；实际类型、可赋值性、地址、线程、帧、owner 在预览／应用重新检查；声明 RAM，寄存器驻留只接受物理 frame 0；const／volatile／AP 路由无隐式回退 | 本机 GCC/GDB 8 阶段通过；MI 故障夹具与键鼠测试；128 位表达式仅纯单元验证；实板未执行 |
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
