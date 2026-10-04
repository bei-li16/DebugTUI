# 寄存器功能实现进度

本文件记录开发分支上的实际实现，配合 [开发 TODO](registers-development-todo.md) 使用。当前仍是未发布的开发版本；下述软件验证不能作为芯片或 OpenOCD 实板能力证明。

## 已实现并经过本地验证

- 内置 cortex-m4、cortex-r52、cortex-r52+ 目录及可覆盖的用户目录。R52+ 目录目前只包含与 R52 共用的描述，尚未据 R52+ 专用手册确认差异。
- 目录版本、大小、组引用及循环、重复 ID、读取编码、位宽、字段和别名校验。原始值使用精确的 32/64/128 位十六进制字符串，支持非连续 CPSR IT 字段和枚举。
- 项目显式目录优先于 CPU 预设；用户同名 CPU 目录优先于程序内置目录。芯片的用户 CPU 关联优先于已知芯片的内置关联；项目显式选择可以覆盖关联。CPU 与目录都未指定时保留原有动态 GDB 列表。
- Setup 的 CPU 选择、目录浏览、来源显示、取消及保存；路径相对声明它的项目或工具配置解析。升级初始化用户 `profiles/registers/` 目录，不复制会长期遮蔽新内置版本的默认文件，也不改写客户文件。
- System Regs 树的 Core/SIMD/System 分组、字段展开、搜索、筛选、目标／全部定义视图、宽／窄布局以及描述。分组和未读取项目不触发目录扫描式读取；可见条目按批请求，最多 128 项、单个在途请求。
- `registers_list` 和 `registers_read` JSON 接口。逐项隔离 GDB 读取失败，保留名称列表中的空位及原索引；配置目录时停止沿用旧自动寄存器批量刷新。
- `Snapshot.registers` 保留原有结构，投影已经按需获取的 Core GDB 值；新增 `register_samples` 提供精确值、状态、原始原因、归属、来源和时间。继续运行、换停止点、会话或相关栈帧后旧结果失效。
- core/cluster/chip 显式拓扑；未知 cluster 不根据核心编号猜测。实现条件为 Yes/No/Unknown，配置事实的来源标为 configuration，不把目录存在或访问失败当作硬件已实现／未实现。
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

64 位 CP15 目前仅在 GDB 已暴露对应具名寄存器时读取完整原始值；未暴露时明确报告 reader unsupported，绝不用两次任意 MRC 拼接。32 位 CP15 命令、银行名称和可选寄存器需要与实际 OpenOCD 构建核对。未知浮点／MPU／GIC 条件不自动轮询。客户端未发送切换 CPU 模式或使能 FPU 的命令；银行和浮点后端的内部行为尚需确认，不能据此宣称后端无副作用。

上述通道经过隔离的 MI/TCL 软件夹具验证，尚未执行上板验证。用户已指定本任务不执行上板测试；交付仍需准备可执行的相应用例并明确记录未执行。

## 完整任务仍需完成的部分

寄存器视图偏好持久化及浮点／向量格式；实际身份与能力采集；64 位后端能力、MPU/PMU 选择器事务和区域视图；完整 GIC 物理／虚拟、Debug、STM 配置状态目录及板级映射；变量的位域／引用／特殊浮点及 128 位后端验收、64 位 MMIO writer 和其余写入类别的跨面板编辑；系统／银行／浮点 writer、一次写／解锁／自清零策略；全部延后上板用例、完整文档和发布构建、安装、推送及 Release。不得因基础框架或部分 writer 通过测试而把完整 TODO 或 Goal 标为完成。

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

2026-10-04 变量写入批次：256 项单元测试、27 项集成测试通过，2 项环境测试 ignored；严格 Clippy 通过。新增精确类型赋值、根存储先检查、const／volatile／优化掉与不可赋值拒绝、原 GDB 函数调用策略恢复；地址／类型／权限／帧变化拒绝旧草稿，发送后错误／超时／断连仅一次赋值并准确分类，回读失败保留 accepted，清理失败要求重连。Locals 支持成员树、格式、键鼠展开及跨帧编辑，树收起不访问目标。真实本机 GDB 的 8 阶段报告位于 `artifacts/variable-write-native-1791113030953-3eec4e55/`，原工程 SHA256 保持一致。

新增 `scripts/test-variable-write-hardware.cjs` 和 `tests/fixtures/variable-write-board.example.json`：默认 4 skipped；指定独立暂停夹具、核心、栈帧、类型路径、布局及邻接符号后，执行取消、单次赋值、独立地址／RAM 回读、哨兵和成功后的显式恢复。真实 DebugTUI 二进制与 MI 软件夹具已通过 5 阶段，报告 `board_tests_executed=false`。需按实际工程分别准备 core0/core1、双核共享 owner、调用者帧及各成员用例；不将软件夹具记作上板验收。

2026-10-04 Core 写入批次：247 项单元测试、10 项本地集成测试通过，2 项 ignored；严格 Clippy 通过。新增精度／特殊写规划、SVD 元数据、独立 writer、真实 UI 键鼠与窄布局、失败保留草稿、MI 单次写／取消／权限及线程变化／上下文失效／准确结果状态／无重试。实板 Core 用例默认生成 skipped；全流程通过本地 MI 夹具，报告 `board_tests_executed=false`，未执行板卡测试。其他写类别和完整 TODO 仍未验收，未升版或发布。

2026-10-04：`cargo test --locked` 通过 213 项单元测试和 2 项本地集成测试，2 项依赖外部环境的既有测试保持 ignored；`cargo clippy --locked --all-targets -- -D warnings` 通过。集成夹具确认连接及停止时无额外寄存器读取、稀疏名称索引正确、单项失败不影响后续 64 位值，以及旧值在继续运行后标为 stale。服务测试覆盖并发串行化、独立服务、共享故障隔离和恢复。

2026-10-04 后续回归：220 项单元测试和 3 项本地集成测试通过，2 项既有测试 ignored；严格 Clippy 通过。新增检查覆盖 Setup 通道草稿取消／保存、核心限制和窄窗口操作、换芯片／会话／栈帧后旧监视绑定失效、总线 64 位高位精确输出，以及寄存器读取途中收到异步 RUNNING 时停止批次并丢弃新值。

2026-10-04 Memory 面板回归：230 项单元测试和 5 项本地集成测试通过，2 项既有测试 ignored；严格 Clippy 通过。新增 TCP 夹具验证明确核心路由、64 位地址、地址顺序及拒绝无效范围／上下文；MI 管道夹具验证多块连续内存响应、读取中 RUNNING 通知，以及 Scope All 下仅读取当前核并拒绝另一核的旧上下文。界面测试覆盖窄窗口、草稿取消、逐芯片／核心设置、单个在途读取、各类迟到响应失效和失败不重试／不回退。功能覆盖矩阵已关联目录与跨面板通道软件证据。未连接板卡，未升版、安装或发布。
