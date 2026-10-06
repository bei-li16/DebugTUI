# 只读系统寄存器版本：冻结范围与验收清单

2026-10-06。开发分支 `codex/register-debugging`，起点 `b493a7f`，源码基线 0.9.3。该清单落实新的 Goal，取代旧 TODO 全部 71 项作为本次完成条件；旧记录保留作追溯。36 项以完整软件验收为计数边界。硬件用例准备属于软件交付，执行上板不属于本目标。

核心参考文档（保留绝对路径）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

原计划与证据：`G:\Data\GitFiles\DebugTUI\docs\registers-development-todo.md`、`G:\Data\GitFiles\DebugTUI\docs\registers-development-status.md`。执行目标来自 `C:\Users\18283\.codex\attachments\f1efd719-c8ab-4c6b-a0e0-99dce1bcda2f\goal-objective.md`。

## 开始时审计

- `git fetch origin` 成功；HEAD 与远端开发分支无领先/落后，均为 `b493a7f9c7e52a36c1c3a977b6e14970f65c3455`。只有用户新增的 `tui-debugger-spec.md` 未跟踪，保留不改、不代为提交。
- 已有目录、树/字段界面、采样来源、生命周期、owner 路由及 OpenOCD 保存恢复可复用。旧软件验收最近为 609 测试通过，不作为新范围已经完成的证明。
- 确认缺口：每核目录、描述继承与手册元数据、M 公共系统寄存器及 ID 驱动、CorePrivate、按 reader 区分运行态、R52 普通 CP15/MPU/selector 当前 Debug 权限一致性。
- R52 TRM 是 `100026_0104_01_en` Issue 01，718 页。指定的 `Armv8-R AArch32.pdf` 是 `DEN0130_0100_en` 架构介绍，26 页；不得把它当完整指令/权限编码手册。需要细节时使用 TRM 引用的架构正文并记录来源，不丢失这三个核心引用。
- 完整 Banked 低 EL、VFP 写入、Bao/Guest、完整 Timer/PMU/GIC/STM、Trace 和变量/内存/外设/sysreg 写入列为后续。已有代码保留，必要时隔离，不继续扩展。

## 冻结的 36 项 feature

| 状态 | ID | 验收边界 | 必需软件证据 |
| --- | --- | --- | --- |
| [x] | A01 | 每核 CPU 选择与旧默认兼容 | 配置、实际 worker 与切核界面测试 |
| [x] | A02 | 每核目录选择、空值及声明文件路径 | 路径/优先级/损坏文件/CLI 测试 |
| [x] | A03 | 每核访问参数继承、覆盖及严格校验 | 路由分离、未知字段/错误参数零 I/O 拒绝 |
| [x] | A04 | 配置层及有效来源可见 | Setup/Status 各核有效值与来源测试 |
| [x] | A05 | 公共定义继承和循环/深度检查 | 有效继承、缺父定义、循环/超深拒绝 |
| [x] | A06 | 显式 override 及继承来源 | 重名拒绝、显式覆盖、父文件来源 |
| [x] | A07 | reset/source/confidence 元数据 | 合法/缺失/冲突、未知复位值和可信度校验 |
| [x] | A08 | 结构化实现/访问条件 | 存在性、NeedHalt/Enable/权限与副作用检查 |
| [x] | A09 | 归属未知及既有 reader/alias 复用 | core/cluster/chip/unknown 与别名隔离 |
| [x] | A10 | 3～5 个代表性定义样例 | schema/字段/来源/条件/副作用专项 |
| [x] | B01 | M3/M4/M7 公共加增量目录 | 内置加载、继承、型号差异及离线生成一致性 |
| [x] | B02 | SCB 与故障字段 | 常用状态/控制及 CFSR/HFSR/MMFAR/BFAR 定义 |
| [x] | B03 | NVIC 与优先级来源 | 动态 bank/优先级位、无依据 Unknown、不写探测 |
| [x] | B04 | SysTick 与读副作用 | CTRL/LOAD/VAL/CALIB、手动读与自动零 I/O |
| [x] | B05 | M MPU 与 region 读取 | TYPE/CTRL/RNR/RBAR/RASR、容量及保存恢复 |
| [x] | B06 | Debug、DWT/FPB 身份容量 | 不轮询 DHCSR、不自动使能、门控原因 |
| [x] | B07 | M4 FPU 配置与身份 | CPACR/FPCCR/FPCAR/FPDSCR/MVFR、GDB regfile 复用 |
| [x] | B08 | M7 cache/TCM 配置 | TRM/CMSIS 有据定义、维护命令不执行 |
| [x] | B09 | M ID 探测与动态实例 | CPUID/ICTR/MPU_TYPE 等成功/缺失/非法结果 |
| [ ] | B10 | CorePrivate 和异构双核隔离 | 同 PPB 地址经不同核路由、缓存与失败隔离 |
| [x] | B11 | 按 reader 运行态读取 | 安全 MMIO 正向、sysreg/GDB NeedHalt 拒绝 |
| [ ] | C01 | R52 常用身份/控制完整描述 | 规定寄存器编码、位宽、主要字段及页码 |
| [ ] | C02 | R52 MPU 完整描述/容量 | EL1/EL2、MAIR 与有效 region 范围 |
| [ ] | C03 | 当前 Debug 权限一致性 | 普通 CP15/MPU/selector 新鲜 EL、trap、成功与拒绝 |
| [ ] | C04 | 后端保存恢复和故障隔离 | scratch 回读、取消/错误、无盲目重试 |
| [ ] | C05 | selector 单事务 | 保存选择读取恢复、失败不发布部分值 |
| [ ] | C06 | 低 EL 与 R52+ 支持边界 | Unknown/Restricted/Unsupported 及正常成功路径 |
| [ ] | D01 | 既有寄存器界面与格式复验 | 分组/列/字段/枚举/说明/格式/变化 |
| [ ] | D02 | 状态、原因与旧值来源 | 各失败类别、灰显、失败非零、原来源保留 |
| [ ] | D03 | 全部生命周期边界 | 核/帧/运行/暂停/重连/共享 owner |
| [ ] | D04 | 按需 I/O 与只读边界 | 未启用不读、不扫描、不持久写控制 |
| [ ] | E01 | 软件入口及硬件用例准备 | MI/Tcl/实际 CLI、可执行用例、上板 SKIPPED |
| [ ] | E02 | 配置示例、用户文档及后续清单 | 新字段/有效值/支持限制和三份绝对引用 |
| [ ] | E03 | 发布前完整回归与静态检查 | cargo/仓库静态检查、必要功能套件全通过 |
| [ ] | E04 | 升版、构建、打包及一致性 | 隔离安装/版本/附件/对应源码核对 |
| [ ] | E05 | 非主分支提交、tag、Release | 最终 SHA、tag 目标、已发布附件校验 |

状态起点 **0 完成 / 36 未完成**；已有成果会在本清单对应测试复验后计入。不得仅因旧记录或部分实现勾选。后续每轮在此记软件证据、完成数和提交推送结果；不以等待上板延长 Goal。

## 迭代 1：每核选择与实际路由

2026-10-06，完成 A01/A02/A03，累计 **3 完成 / 33 未完成**。

- 新增可选 `cores.registers`，省略字段继承有效项目默认，显式空值清除选择；按声明项目解析目录路径，逐核严格校验。芯片 topology 仍保持全局。Coordinator、UI、Setup 与 Status 使用相应核的有效配置。
- 新增 7 项配置/worker/界面/预览单元测试。完整单元回归 **431 通过、0 失败、2 ignored**。回归中发现未保存 CPU 草稿预览优先级问题，已修复并保持原测试断言。最终记录：`artifacts/readonly-core-validation-20261006-105719.log` 的单元结果。
- 实际 CLI 配置脚本扩大至 **12 case，12 通过**，覆盖双核不同目录与事实、空值回退、未知字段/非法命令/缺失文件在连接前拒绝，保留客户文件。记录：`artifacts/readonly-core-integration-20261006.log` 中 CLI 测试结果。
- 新增实际 Coordinator/MI/Tcl 双 TCP 端点测试，检查两个非连续核心的 endpoint、target、返回值与 owner/provenance。夹具日志和传输转义断言修正后通过；记录：`artifacts/readonly-core-routing-20261006.log`，详细凭据在 `artifacts/per-core-routes-*/evidence.json`。本地服务器只验证协议和路由，不模拟 ARM 指令执行。
- 严格 Clippy 全 target 检查通过：`artifacts/readonly-core-clippy-20261006.log`。新增硬件用例 CONFIG-H09/H10，状态 SKIPPED；没有上板测试，也未重建未改动的 OpenOCD。
- A04 仅新增各核有效 CPU/目录来源/路由预览，完整配置层来源尚待实现，未计入。下一轮先完成公共描述继承、明确覆盖和 3～5 个代表定义，再迁移目录。提交 SHA 与推送结果由本轮最终输出报告。
- 提交 `7a628e2c1f3f9bf4fa3b50fa62a32798471d5bc6` 已推送，远端 SHA 一致。

## 迭代 2：公共描述继承与来源

2026-10-06，完成 A05/A06/A07/A10，累计 **7 完成 / 29 未完成**。

- 实现名字/相对路径父目录、三层及输入资源限制、循环/歧义拒绝、完整显式覆盖。新增 version 2 来源校验、复位值、可信度和声明的硬件验证元数据；version 1 缺失项仍是 Unknown。来源诊断保存在运行态及 `registers_list.definition_origins`，不写入持久化 TOML。
- 五个定义样例 MIDR/MPUIR/PRBAR0/EDPRSR/Alias 验证字段、MPU 容量条件、副作用与父文件来源。已核对 R52 TRM 指定页和架构正文；没有批量扩大正式目录。说明见 [继承与描述来源](register-catalogue-inheritance.md)。
- 完整单元回归 **439 通过、0 失败、2 ignored**；最终路径及元数据专项 **7 通过**。新增 Status 小/中/大窗口来源滚动与零 I/O 测试通过。实际配置集成 **2 测试通过**（12 CLI case 及双 TCP 路由），继承实际入口 **1 测试通过**（3 case，含八类连接前拒绝）。
- 回归中修复：来源诊断膨胀持久化目录并破坏快照往返、CPU 帮助行遗漏、窄屏 CPU 文本可见性、Windows 并发 fixture 重名，以及原有文件错误前缀兼容。保留原测试断言，没有提高 4 MiB 单文件限制。
- G 盘空间不足后，改用 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly` 构建，并将 Node 测试证据放到其 `evidence` 子目录；原 target 未删除。日志：`readonly-catalogue-full-final-20261006.log`、`readonly-catalogue-unit-final-20261006.log`、`readonly-catalogue-integration-final-20261006.log`、`readonly-catalogue-path-final-20261006.log`、`readonly-catalogue-clippy-20261006.log`。完整回归后的最终改动仅为文件错误前缀和显式 `.toml` 父文件识别，各自已专项/实际入口复验；Clippy 调整仅合并等价条件。
- 新增 CONFIG-H11，环境验收保持 SKIPPED，没有执行上板测试或重建 OpenOCD。A04/A08/A09、M 公共目录/动态探测/CorePrivate/运行态、R52 权限统一及发布仍待完成；下一轮推进结构化条件与 M 公共目录。提交 SHA 与推送结果由本轮最终输出报告。

## 迭代 3：结构化条件与私有核路线

2026-10-06，完成 A08/A09，累计 **9 完成 / 27 未完成**。

- `present_if` 使用有限的寄存器字段比较；`access_rule` 分离 NeedHalt、NeedEnable 和当前 Debug min_el。配置不能伪造观测字段，未知存在性/使能/权限和 unknown scope 在数据访问前拒绝，手动与 Alias 均不能绕过。样本需当前物理 owner/context、完整值和已响应的实际请求；最新失败不复活旧 probe，早于当前证明的迟到样本不覆盖新依据。条件保留原源样本/路线/时间和共享 owner。
- 增加 CorePrivate 绝对 PPB reader，复用现有内存通道；显式每核 ppb binding、零 base、专属核心通道、匹配 target，并拒绝另一核复用同 endpoint/target。显式 GDB memory 使用已知且独立的 worker endpoint，不透明连接变更或多个核心共享该 endpoint 时拒绝。API、只读矩阵与界面使用同一策略；没有重写 DCRSR/DCRDR。
- 新增 8 项模型/图/路线/观测单元测试和 1 项 UI 自动队列测试。最终完整单元 **448 通过、0 失败、2 ignored**；本批选择的 12 个相关集成套件共 **64 项最终通过**，包含八类 reader/来源、取消、配置/继承实际 EXE、共享/生命周期及 MMIO。新 policy 集成 3 项覆盖两个 TCP target、真实 MI memory 和实际 EXE；其中 EXE 检查一项有效配置及五种连接前拒绝。日志位于 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly`：`readonly-policy-regression-20261006.log` 和 `readonly-policy-final-20261006.log`。前者新 GDB 夹具因缺 target.endpoint 失败，补齐后在后者 3/3 通过；后者还补齐首次失败后未运行的来源/共享套件。不声称未选择的集成套件已运行。
- 自检修正缓存父节点绕过图深度限制，增加 64/66 层边界，并在门禁前清空上一次实际 access，避免拒绝后沿用别项请求。`cargo fmt --all --check` 与严格 Clippy 全 target 检查通过，Clippy 记录在 `readonly-policy-clippy-20261006.log`；首次仅遇到测试引用切片的 Clippy 提示，等价改为 from_ref 后复验通过。
- 新增 [策略与来源说明](register-structured-policy.md) 及七项 [环境用例](../tests/cases/register-structured-policy.md)，全部 SKIPPED。用户请求的新 [Goal 文本](registers-goal-objective.md) 一并保存，三个核心绝对引用保持。
- 这里完成公共策略模型，不等于 B11 运行态入口或 C03 R52 生产证明已完成。当前通用 min_el 因缺后端证明仍为 Unknown；NeedHalt=false 不降低现有请求/通道限制。B10 虽有实际双 TCP/GDB 路线证据，正式 M 目录、ID 驱动及完整异构缓存验收尚未完成，继续不勾选。下一轮完成 M 公共/增量目录与 ID 接入，再验证 B10/B11。提交 SHA 与推送结果在本轮最终输出报告。

## 迭代 4：Cortex-M 公共目录与增量

2026-10-06，完成 B01/B02/B04，累计 **12 完成 / 24 未完成**。前一批迭代 3 已提交并推送为 `aa0d503b0748075c93c49ddba9b2a9209510278b`，远端 SHA 一致。

- 新增 `armv7m-common` 与 M3/M4/M7 内置继承入口，Setup 支持三个型号。保留 23 项 GDB 通用定义及其独立 writer；M4/M7 浮点继续复用 GDB/Alias，并增加真实 MVFR0 字段的存在性条件。新 PPB 项使用 CorePrivate，没有新增 writer。公共文件变化可传播到三个型号，型号差异保留明确 override 来源。
- 固定 CMSIS-Core 6.1.0 的实际提交 `b0bbb0423b278ca632cfe1474eb227961d835fd2`，原始三份头文件与 LICENSE 保留。逐文件与不可变 upstream URL 的 SHA256 核对一致，证据 `readonly-m-cmsis-source-20261006.log`。离线转换校验源摘要、有限数字表达式、结构偏移与字段重叠；生成检查覆盖六个目录，R52/R52+ 产物未改变。
- SCB 覆盖常用状态及故障位。生成中发现 CMSIS 的 M3 VTOR revision 分支、AIRCR 读写键与拼写别名、CFSR/MPU 聚合 mask；明确排除重叠，未知 M3 VTOR 布局保留原始值并标 `fields_missing`，不按最后一个宏猜测。FP_CTRL 的非连续容量字段来自 DDI 0403E.e §C1.11.3。
- SysTick CTRL 和 DHCSR 手动单次读，自动请求零内存 I/O；SysTick 架构明确区分软件读清与 debugger read 保留 COUNTFLAG，当前策略因访问属性未验证而保守，不宣称所有调试器读取都会读清。三个型号的实际 Coordinator/MI 流程核对故障值、两个手动状态值和精确三次读取，无写入或运行控制。
- 三项新模型专项及三项 Python 转换测试通过。完整单元 **451 通过、0 失败、2 ignored**，本批四个相关集成套件共 **8 项最终通过**（配置 2、继承 1、显示 1、policy 4）；严格 Clippy 全 target 与格式检查通过。日志均在 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly`：`readonly-m-catalogues-final-20261006.log`、`readonly-m-catalogues-policy-final-20261006.log`、`readonly-m-catalogues-clippy-20261006.log`。完整回归首次四个失败来自旧测试的 CPU 索引、声明来源及浮点存在性预期，更新后单元通过；新传输测试只因要求带引号的日志断言失败，改为严格核对实际参数/地址/次数后 4/4 通过。生产门禁没有降低，最终修正仅涉及测试断言。
- [目录与生成说明](register-cortex-m-catalogues.md) 及四项 [环境用例](../tests/cases/register-cortex-m-catalogues.md) 已准备，全部 SKIPPED，没有上板或重建 OpenOCD。NVIC 优先级来源、M MPU region 事务、完整 ID Probe、FPU/cache 能力验收、异构缓存和运行态入口仍未完整，B03/B05～B11 不勾选。下一轮接入 M ID/容量观测与优先级来源，再完成相应动态功能。提交 SHA 与推送结果由本轮最终输出报告。

## 迭代和发布约束

每轮先专项验证；阶段边界和发布前完整回归。无新增改动或疑点不重复相同全量测试，不重建未改动的 OpenOCD。每轮形成可审查改动后提交并推送开发分支。只读允许必须恢复的 scratch/selector 临时写入，不自动使能或改变模式、权限。全部 36 项验收后，更新版本并按仓库发布流程发布非主分支 Release，注明仅软件验证及待执行硬件用例；完成后结束目标。


## 迭代 5：M ID、动态容量和 NVIC 声明来源

2026-10-06，完成 B03/B09，累计 **14 完成 / 22 未完成**。迭代 4 已提交并推送为 `e904383db607cba54e5c7aa8d50183b167de6910`，远端已核对一致。同步修订用户要求的新 Goal 起点记录，保留三个绝对引用，不变更冻结范围。

- M3/M4/M7 使用同一生产 capability 入口，复用 worker/服务锁、当前 frame/thread 检查及 CorePrivate MI/Tcl memory。CPUID 匹配适配型号才读取 ICTR/MPU_TYPE/DEMCR/FP_CTRL 等；DWT 先证明 TRCENA，FPU 仅核实的正向 MVFR 编码给出能力。没有新增注入后端、DCRSR/DCRDR 调试器或 writer。原始样本保留，非法容量不授权动态 bank/MPU 项，配置不能伪造观测；较新的失败或身份变化撤销有效事实，早期 probe 不复活旧值。
- `probe.nvic` 保留 SVD CPU/来源/优先级/IRQ 列表及配置声明；优先有效 SVD，再用 3..8 的显式配置，缺失 Unknown。CPU 错配和越界 IRQ 报告冲突，不据此增加实例。NVIC Status 展示优先级来源和匹配 IRQ 名称，过期上下文不复用；这些来源是声明，不冒充 ID 观测。Scope All 仍只探测选中核，异构 worker 不串 owner/端点/容量。
- 新增 M 模型六项、Status 一项单元测试，M 生产 MI/Tcl 六项集成；三种型号分别通过实际 MI，异构 M4/M7 经两个不同端点，实际 TCP Tcl 检查精确 target/address/32-bit/count 与七次只读请求。正向、未知/错配/缺失/非法 ID、未使能、越界零 I/O、配置/SVD 冲突、运行通知中止和较新身份撤销均有独立断言。绝不以夹具字节证明 ARM 实板。
- 单元回归 **457 通过、0 失败、2 ignored**，当时选择的相关集成 **25 通过**；日志 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\readonly-m-probe-regression-final-20261006.log`。随后新增 FPU 非法编码单元及 Tcl/坏容量集成、收紧 FPU DP 编码，最终 M 专项 **6/6** 和 M MI/Tcl **6/6**，相关集成合计 **27 项最终通过**。最终日志为同目录 `readonly-m-probe-final-focused-20261006.log`、`readonly-m-probe-mi-complete-20261006.log`。不宣称后续未选的全仓集成已运行，也不把重复执行算新 case。
- 初次集成遇到 Windows 时钟目录碰撞，改为进程内唯一计数而保留原读取次数断言；新增 UI 测试错误使用 key helper，改为实际 KeyEvent；Tcl 新夹具的 accepted socket 继承 nonblocking，明确还原 blocking 后通过。初始失败日志保留在同目录 `readonly-m-probe-mi-20261006.log`、`readonly-m-probe-regression-20261006.log`、`readonly-m-probe-final-mi-20261006.log`。这些是夹具/编译修正，没有放宽生产协议或跳过失败。
- 严格 Clippy 全 target 最终通过（16.80 秒）：`readonly-m-probe-clippy-final-20261006.log`；首次仅提示 String 的冗余转换，等价去除后复验。`cargo fmt --all --check`、diff 检查、六份离线目录生成一致性和三项 Python 生成测试通过。正式目录未增加新的寄存器类别，未重建不变的 OpenOCD。
- 文档 [M 身份、容量与 NVIC](register-cortex-m-probe.md) 和四项 [硬件 case](../tests/cases/register-cortex-m-probe.md) 完成，硬件全部 SKIPPED，来源引用保留。B05 的 region 事务、B06/B07/B08 整体模块验收、B10 生命周期和 B11 运行态仍未完成；已有 ID/目录/路线成果可复用，不能把它们重复开发。C01～C06、D01～D04、E01～E05 继续按清单推进。提交 SHA 与推送核验结果由本轮最终输出报告。

## 迭代 6：M MPU region 单事务与恢复

2026-10-06，完成 B05，累计 **15 完成 / 21 未完成**。迭代 5 已提交推送 `52c78f923111fc412f5fc12b7f52a3732af9af14`，开始本轮时再次核对本地/远端一致；用户要求修订的 [Goal 文本](registers-goal-objective.md) 随本轮保存，保留 14/22 的修订起点及实时清单优先规则。

- 扩展既有 `registers_mpu` / `:mpu` 总览，M3/M4/M7 采用 `bank=m`。重用 CMSIS 定义、CorePrivate binding、服务 lease 和实际 MI/Tcl 传输；一个显式 target Tcl 事务重新核对 CPUID/TYPE/停止状态，保存 RNR、逐个选择/回读/读取 RBAR/RASR、恢复并验证，完整响应及最终线程/帧通过后才发布。只临时写 RNR，没有 MPU 配置写入、全局 target 切换或新 CPU 注入器。
- 实际观测的 0/8/16 容量决定实例。没有 MPU 时只核对身份/TYPE，CTRL/selector 无样本，regions 为空；不制造零控制值。无法证明容量、修改固定 reader/权限/副作用定义、缺少独立 Tcl/AP 路线时在目标 I/O 前拒绝。GDB memory 保留普通当前 RNR 读取能力，不能冒充此 bank 事务。
- Snapshot `register_mpu` 保存 indexed bank，复用每项 Sample/Reader/owner/Context/请求区间和来源，不覆盖普通当前 RNR 样本。失败/取消不发布部分值，旧 bank 保留原值、原时间和原来源并 stale；新的非法 TYPE、运行/换帧/重连等边界撤销有效性。恢复失败、损坏/断开的响应隔离服务为 FAULT，没有自动重试。M4/M7 的 core0/core2 经不同 target，Scope All 仍只读取选中核。
- 第一批单元/集成回归 **461 单元通过、2 ignored**；相关身份、M ID、策略及共享集成 **29 通过**。完整 selector 套件 **90 通过、1 新夹具失败**，其中原有 81 项通过；新测试把零 I/O 的空 trace 当已初始化列表。修正 trace 初始化，并让自定义 M 目录清除已展开的 extends；GDB-only case 增加真实 MI 身份/容量基线，使其确实到达 unsupported-route 门禁。没有降低断言或跳过失败。回归日志 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\readonly-m-mpu-regression-final-20261006.log` 保留原失败，不把它标为全部通过。
- 随后补齐 TYPE 撤销 bank 的规则及用例，最终 MPU 模型/UI 专项 **10/10**，M worker/Coordinator/真实 Tcl/实际 EXE 专项 **11/11**。最终日志为同目录 `readonly-m-mpu-unit-final-20261006.log`、`readonly-m-mpu-cases-final-20261006.log`。之后的生产改动仅为 M bank 缓存检查及未采样 selector 的空值表达，原 R52/空 bank 路径不变；没有宣称本轮运行全仓所有集成。实际 EXE 环境驱动正向和错误独立基线拒绝均通过。
- Tcl 夹具最初只服务每连接一帧；改成持久连接后又暴露阻塞新连接的问题。最终同时接收多个连接的完整帧，在唯一 Tcl 解释器中顺序执行，原 81 项 selector 回归证明已有流程仍适用。保留 `readonly-m-mpu-first-20261006.log`、`readonly-m-mpu-focused-20261006.log`、`readonly-m-mpu-focused-final-20261006.log`、`readonly-m-mpu-final-20261006.log`、`readonly-m-mpu-multiplexed-20261006.log` 的编译/夹具失败和最终恢复，不伪造软件夹具为 ARM 实板。
- `cargo fmt --all --check`、严格 Clippy 全 targets（17.15 秒）、diff 检查、六份离线目录生成一致性及 Node 语法检查通过。Clippy 日志 `readonly-m-mpu-clippy-20261006.log`。未修改或重建 OpenOCD。
- [M MPU 文档](register-cortex-m-mpu.md)、用户指南及三项 [环境 case](../tests/cases/register-cortex-m-mpu.md) 完成；驱动 `scripts/test-m-profile-mpu-hardware.cjs` 默认三个阶段全部 SKIPPED、零目标 I/O，报告位于上述构建目录 `evidence/m-profile-mpu-hardware-1791266088629-c6f06ef9`。所有硬件未执行，verified 未升级。三个核心绝对参考路径均保留。
- 剩余 A04；B06/B07/B08/B10/B11；C01～C06；D01～D04；E01～E05。下一批验收 M Debug/DWT/FPB 与 FPU/cache 模块整体行为，复用已有目录和 ID；完整异构生命周期、运行态、R52 当前 Debug 权限和 Release 仍未完成。最终提交 SHA 与远端核验由本轮输出和证据 manifest 记录。

## 迭代 7：M Debug/DWT/FPB 与 FPU 整体验收

2026-10-06，完成 B06/B07，累计 **17 完成 / 19 未完成**。开始时核对 HEAD 与远端开发分支均为 `e4bd648f464abb46fadd45f64484fdf45a6edb2a`。本轮一并保存用户要求重新生成的 Goal 文本，其 15/21 是修订起点，本清单为实时状态。

- 复用已验收的 M 目录、ID 与 CorePrivate。实际 Coordinator/worker/MI 在 M3/M4/M7 验证 DHCSR 自动零 I/O、手动单次读取，DWT 未使能/未知/已使能读取及新 DEMCR 关闭/失败撤销容量；FPB REV 0/1、零容量、分离 NUM_CODE 的高低段、未知 revision 和失败均有独立原始响应与断言。没有启用 DWT、FPB 或写比较器。
- 修正 M4/M7 CPACR 缺字段：固定 CMSIS 供地址，位域和枚举分别来自 DUI 0553A §4.6.1/Table 4-50 物理第 264 页和 DUI 0646B §4.7.1/Table 4-59 物理第 287 页。保留 CP10/CP11 不一致与保留编码；不填未知 reset，不添加 writer。离线生成器及输出一致，4 项 Python 转换检查通过。
- M4/M7 FPU 配置和 GDB D/S/FPSCR 实际读取验收：禁止/仅特权/完全访问 CPACR 的软件情形不自动改变使能；GDB 停止帧与外部读取独立检查。非对称 64 位、NaN payload、D15/S30/S31 和 FPSCR 精确保留，同批父 D 与 S 别名复用一次来源。无效/缺失 MVFR0、伪造声明、只有 S 名而缺 D 父项、错误响应和旧来源保留分别检查。软件模型不宣称任何实板必然在所有权限配置下可读。
- `readonly-m-modules-final-20261006.log` 在 C 盘 build-cache：8 项 M profile 模型单元、1 项新增 CPACR UI（35×12/80×24）和完整 12 项 `m_capability_access` 集成均通过。其中 CPACR 模型在两个过滤命令中重复出现，唯一单元/UI 是 9 项，不能合计成 10。测试直接运行生产解析、字段、UI、Coordinator/worker/MI；不是 ARM 执行模拟。新增 6 项集成中的实际 EXE 驱动通过五阶段加 cleanup，故意错误独立 D0 baseline 被正确判失败。
- 严格 `cargo clippy --locked --all-targets -- -D warnings`、`cargo fmt --all --check`、六目录离线生成一致性、Node 语法和 `git diff --check` 通过。没有为未改动 OpenOCD 重建平台，没有声称本轮完成全仓回归；前序证据保持适用。初次新测试中暴露的 M7 source 页码缺失和 retained-origin 包装断言已按实际 schema 修正后通过。
- [模块说明](register-cortex-m-modules.md)、[五项环境 case](../tests/cases/register-cortex-m-modules.md)、显式运行驱动和独立期望值模板完成。默认报告 `evidence/m-profile-modules-hardware-1791267355736-4dfa8b96/report.json` 为 0 passed / 0 failed / 5 skipped，目标零 I/O、`board_tests_executed=false`。没有上板或自动升级 verified。
- 剩余 A04、B08/B10/B11、C01～C06、D01～D04、E01～E05，共 19 项。下一批优先 M7 cache/TCM 及 selector，再接通按通道的运行态和完整异构隔离；R52 当前 Debug 权限及最终发布仍待完成。提交 SHA 和推送核验结果在本轮最终输出报告，证据 manifest 保存最终提交。

## 迭代 8：M7 cache/TCM 与 CSSELR 事务

2026-10-06，完成 B08，累计 **18 完成 / 18 未完成**。开始时核对本地/远端开发分支均为 `8b7aa9060c13eb5fca0dfff8e6dcfded940552d4`。上一目标轮次重整 Goal 文本、同步已提交的 B06/B07 和 17/19 起点，属于目标文档状态变更；本轮复用已有未完成的 M7 cache 代码，补齐入口、生命周期、专项及交付文档。Goal 文本的 17/19 保留为重新生成时快照，本清单为实时计数。

- M7 Probe 新增实际 CLIDR/CTR 观测，CPUID 型号核对、严格编码、完整同核来源及最新失败撤销沿用 M policy。CLIDR 的 I/D 实现位依据 DUI 0646B §4.5.1/Table 4-40 物理第 269 页补齐 CMSIS 缺项；CCSIDR、CTR、CSSELR 记录同手册的准确来源。容量只依据 Table 4-43 的适用编码显示，未知编码保留 raw，不编造 reset/verified。
- `registers_cache` / `:cache` 总览复用现有 MPU 控制器、CorePrivate、服务 lease 和 MI/Tcl。单个显式 target 事务核对 CPUID/CLIDR/CTR 与 halt、保存 CSSELR、选择/回读已实现 I/D、读取 CCSIDR、恢复/回读并复核最终上下文。只临时写 CSSELR，不使能 cache/TCM，不执行维护命令，不切全局 target。普通 CCSIDR 复用该事务返回原选择的 cache；未实现 bank 明确拒绝；无 cache 不访问 selector/CCSIDR。仅 GDB memory 路线在数据 I/O 前拒绝。
- `Snapshot.register_cache` 保存各 bank 的实际 owner/context/类型和完整请求来源。错误/取消/迟到上下文不发布部分值，旧值保留原时间/来源并 stale；恢复/响应不确定 FAULT/隔离，无盲目重试。新非法 CLIDR、运行、frame 与 session/generation 边界撤销 bank；双 M7 同 PPB 地址、不同 target/value/original selector 验证独立缓存。完整 M7＋M4 生命周期和运行态入口仍留给 B10/B11，未提前验收。
- TCM/CACR/AHBP/AHBS 配置复用普通字段树及 CorePrivate；六项原始配置值实际读取通过。九个 WO 维护定义的 manual 请求同样零目标 I/O。打开、滚动、字段显示、关闭和缓存查询不产生目标读取；stale 不继续派生容量/字段，窄/宽窗口验证通过。
- 模型/UI 专项 6 项通过；M profile/MPU 模型回归 11 项、MPU/cache UI 5 项通过，与专项重叠 3 项，合计 **19 项不同模型/UI 测试**。日志均在 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly`：`readonly-m-cache-unit-20261006.log`、`readonly-m-cache-profile-regression-20261006.log`、`readonly-m-cache-ui-regression-20261006.log`。
- 共用 Tcl fixture 扩展后完整 selector 套件 **103/103** 通过（含原有 92 项和新增 11 项），日志 `readonly-m-cache-selector-regression-20261006.log`，323.83 秒。随后去除普通 CorePrivate 请求重复解析内置目录的开销，增加 cache 运行/frame/session 边界用例；最终当前代码 cache 专项 **12/12** 通过，日志 `readonly-m-cache-cases-final-20261006.log`。未受该末尾改动影响的 R/M MPU 路径沿用完整套件证据；没有重复全量或宣称运行全仓所有测试。M 能力集成 **12/12** 通过，日志 `readonly-m-cache-capability-regression-20261006.log`；M7 新增两次 ID 读取的独立期望计数已同步。
- 严格 Clippy 全 targets、fmt、diff、六目录离线生成一致性、5 项 Python 转换检查和 Node 语法通过。最终静态日志 `readonly-m-cache-clippy-final-20261006.log`。初次编译中命令数组长度/父模块私有字段及新 test 的 PathBuf 类型问题已修复；原失败日志保留为 `readonly-m-cache-first-20261006.log`、`readonly-m-cache-cases-first-20261006.log`。没有修改/重建 OpenOCD，没有降低断言。
- [M7 说明](register-cortex-m7-cache.md)、用户指南、[四阶段硬件 case](../tests/cases/register-cortex-m7-cache.md)、默认零 I/O 驱动及独立期望值模板完成。默认报告 `evidence/m7-cache-hardware-1791269363829-0b1b21d0/report.json` 为 0 passed / 0 failed / 4 skipped。当前实际 EXE 软件驱动 `evidence/m7-cache-hardware-1791269867794-32fcbf31/report.json` 为 5 passed；故意错误容量基线 `evidence/m7-cache-hardware-1791269870215-6cb0cab1/report.json` 正确为 3 passed / 1 expected failure / 1 skipped。所有报告 `board_tests_executed=false`，未执行上板或升级 verified；三份核心绝对引用保留。
- 剩余 **A04；B10/B11；C01～C06；D01～D04；E01～E05**，18 项。下一轮优先按实际 reader/通道接通安全运行态，再补完整异构隔离；R52 当前 Debug 权限、最终回归/打包和非主分支 Release 仍待完成。本轮提交/推送 SHA 由最终输出及 `readonly-m-cache-evidence-20261006.json` 记录。

## 迭代 9：按实际 reader/通道的运行态寄存器读取

2026-10-06，完成 B11，累计 **19 完成 / 17 未完成**。开始时开发分支本地/远端均为 `ce3b1cc8ac65a0f7d15dea53cd770be5d7d1ddb8`；上一轮已提交 Goal 修订，本轮复用原工作区未完成的运行态代码并完成入口及验收。

- Registers 的实际共享 UI 调度、手动 Read 和生产 `registers_read` 接通 RUNNING；寄存器/全部 Alias 父项与实际通道都须允许运行态，CorePrivate 沿用核独占路线校验。GDB memory/regfile、借核 reader、需要 stopped proof 的 MMIO/STM、M7 CCSIDR 保护事务仍明确 NeedHalt/路线拒绝，零数据 I/O；Probe/MPU/cache bank 仍停止态。不自动 halt、使能或换后端。
- 新 `running_memory` 样本只接受响应完成、同 context 的实际 AP 区间；停核旧值进入运行态失效，运行值暂停后 stale。读前后复用内存通知/epoch 边界，同批停止/再运行、线程选择、GDB 断线或取消不发布迟到值、不继续后续数据读。失败响应为本次错误，快照/UI 保留原 raw 与 retained-origin。运行 raw 不升级停止态能力/selector 证明，依赖者仍 Unknown/受限。
- UI 进入新运行状态清除上一状态尝试记录，仅可见安全项取样一次；无固定 tick，无启用项不轮询，纯状态弹窗零 I/O。新 1 项样本模型＋4 项 UI 验证真实共享调度、旧状态响应拒绝、NeedHalt 分类与 35×12/80×24 的 AP 来源/区间展示。最终寄存器相关 **204 通过**、UI **186 通过/1 既有 ignored**，交集去重后 **339 项不同通过测试**；`running_` 专项 14 项均包含于该回归，不能额外累加。
- 实际 worker/MI/TCP/Tcl 新 8 项运行态集成及原 M7 运行/frame 边界专项，最终 **9/9**。覆盖 M3/M4/M7 真正数据路径、64 位非原子 MMIO/Alias 共用一请求、错核/停止/GDB-only 通道、手动副作用/WO、错误保留、取消及 stop/run/thread/close 竞争。共享入口相关八套 MI/EXE 回归 **59/59**；共用 Tcl fixture 的 M MPU 回归 **11/11**。未重复全仓/全部 selector，也未重建未改动 OpenOCD。
- 最终日志位于 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly`：`readonly-running-unit-final-20261006.log`、`readonly-running-cases-final-20261006.log`、`readonly-running-register-regression-final-20261006.log`、`readonly-running-ui-regression-20261006.log`、`readonly-running-integration-regression-20261006.log`、`readonly-running-mpu-regression-20261006.log`、`readonly-running-clippy-20261006.log`。严格 Clippy 全 targets、fmt/diff、六目录离线一致性、5 项 CMSIS Python 和 Node/Python 语法通过。
- 初次夹具 Alias 字段/整数类型/私有弹窗字段编译错误已修正；错核 case 补齐合法核声明，错误旧值断言核对真正保留它的状态快照；故障通知在清理前撤销，避免夹具再次强制 RUNNING。旧停止态全请求拒绝断言更新为逐项 NeedHalt、零 access/value，断连仍拒绝；没有降低数据拒绝/来源/取消断言。初次失败记录 `readonly-running-first-cases-20261006.log`、`readonly-running-cases-20261006.log`、`readonly-running-register-regression-20261006.log` 保留。
- [说明](register-running.md)、用户指南、[七项硬件 case](../tests/cases/register-running.md)、四阶段可执行驱动和独立期望模板完成。默认报告 `evidence/register-running-hardware-1791272135129-da5eb158/report.json` 为 0 passed/0 failed/4 skipped；实际 EXE 软件报告 `evidence/register-running-hardware-1791271984566-ec79cb51/report.json` 为 5 passed，故意错误 CPUID 报告 `evidence/register-running-hardware-1791271986230-d9bbf7b5/report.json` 为 3 passed/1 expected failure/2 skipped，测试正确拒绝该基线。所有 `board_tests_executed=false`；未上板、未升级 verified。核心三个绝对路径保留。
- 剩余 **A04；B10；C01～C06；D01～D04；E01～E05**，17 项。下一批完成异构 CorePrivate/生命周期及有效配置来源，再统一 R52 当前 Debug 权限与事务；最终完整回归、升版/打包和非主分支 Release 仍未完成。最终提交/远端核验见本轮输出及 `readonly-running-evidence-20261006.json`，Goal 保持 active。


## 迭代 10：每核有效配置、覆盖来源与冲突解释

2026-10-06，完成 A04，累计 **20 完成 / 16 未完成**。本轮起点本地/远端为 `125fa09f245d34f2d39e7750233b095129199aaa`；前一目标轮次已推送新 Goal 文本。本轮沿用未提交的 A04 草稿，完成自检、回归及验收，不重做已完成 feature。

- 新增只保存在内存的声明来源模型，跟随现有 profile 根、backend、项目与每核解析。CPU 芯片关联单独标默认，不冒充项目或观测；相对目录按声明文件解析；逐个 map key 保留值及覆盖来源，全局空表递归合并、每核 map 整表替换/清除可解释。运行时值与声明不一致时标明 origin unavailable，不借用旧声明授予权限，不持久化来源字段。
- Setup F1、System Regs Status 和实际 `registers_list.configuration` 展示各核配置。配置 CPU 与实际目录 CPU 冲突有明确优先规则；草稿尚未保存可见；每核 CPU/目录编辑保留当前目标观测以解释冲突，memory_access 路由变化撤销旧身份。查看、切核和滚动均零目标 I/O，客户项目/profile 字节不变。原说明与重要诊断位置保留，来源可完整滚动。
- 新增六项模型、两项 Setup、一项 Status Rust 测试，以及四项实际 EXE 用例；配置内层套件由 12 扩至 **16/16**。覆盖 root/backend/project/per-core、空表/清除、非连续核编号、芯片默认、显式 environment、未知来源、错误配置连接前拒绝、窄宽窗口及来回切核。CONFIG-H12/H13 已准备、SKIPPED，没有上板、自动 Probe 或升级硬件 verified。说明见 [配置来源](register-config-sources.md)。
- 完整 `cargo test --locked` **483 单元＋234 集成＝717 通过、2 ignored**，退出码 0；含 selector 112 项、writer 42 项及生产分发 14 内层 case。末尾新增一项环境拒绝测试和相应 EXE 用例，生产来源实现恢复至该完整回归的规则；最终完整单元 **484/484**、配置集成 **2/2**（EXE **16/16**及实际双 TCP 路由）通过。最终树软件证据分批覆盖 **484 单元＋234 集成**，不是单次 718 项执行；此前分发包是软件替换夹具，不是历史正式版升级或最终 Release 的验收。最终发布前 E03 仍须完整回归。
- 严格 `cargo clippy --locked --all-targets -- -D warnings` 通过，18.10 秒；`cargo fmt --all --check`、Node 语法和 `git diff --check` 通过。日志、最终源文件 SHA256、实际 EXE report/产物 SHA256及提交核验存于 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\readonly-config-sources-evidence-20261006.json`。主要日志同目录：`readonly-config-sources-cargo-complete-20261006.log`、`readonly-config-sources-unit-complete-20261006.log`、`readonly-config-sources-configuration-complete-20261006.log`、`readonly-config-sources-clippy-20261006.log`。
- 自检修正来源详情挤走原说明/诊断和旧固定 128 行扫描的断言；改为完整滚动核对，未移除原断言。G 盘空间不足使旧夹具写入失败，测试统一支持 `DEBUGTUI_TEST_ARTIFACT_ROOT` 后在 C 盘回归，不删除用户文件或旧缓存。并发 capability 套件一次超时，增加请求名诊断后独立及 4 并发完整回归通过，未放宽 10 秒约束。后续来源审计曾误认为环境允许 cores；实际加载器明确拒绝 root/backend cores，撤回无用来源扩展，新增严格拒绝用例，保持原配置约束。原始 StorageFull、UI 失败、超时及错误假设测试日志全部保留。
- 剩余 **B10、C01～C06、D01～D04、E01～E05**，共 16 项。下一轮复用 M MPU/cache fixture，补齐真正 M7＋M4 同 PPB 地址的身份、目录、数值、错误和生命周期隔离。R52 当前 Debug 权限统一、最终回归、打包/升级及非主分支 Release 尚未完成；Goal 继续 active。本轮提交与远端 SHA 在最终输出及证据 manifest 记录。
