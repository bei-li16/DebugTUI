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
| [ ] | A04 | 配置层及有效来源可见 | Setup/Status 各核有效值与来源测试 |
| [x] | A05 | 公共定义继承和循环/深度检查 | 有效继承、缺父定义、循环/超深拒绝 |
| [x] | A06 | 显式 override 及继承来源 | 重名拒绝、显式覆盖、父文件来源 |
| [x] | A07 | reset/source/confidence 元数据 | 合法/缺失/冲突、未知复位值和可信度校验 |
| [ ] | A08 | 结构化实现/访问条件 | 存在性、NeedHalt/Enable/权限与副作用检查 |
| [ ] | A09 | 归属未知及既有 reader/alias 复用 | core/cluster/chip/unknown 与别名隔离 |
| [x] | A10 | 3～5 个代表性定义样例 | schema/字段/来源/条件/副作用专项 |
| [ ] | B01 | M3/M4/M7 公共加增量目录 | 内置加载、继承与型号差异 |
| [ ] | B02 | SCB 与故障字段 | 常用状态/控制及 CFSR/HFSR/MMFAR/BFAR 定义 |
| [ ] | B03 | NVIC 与优先级来源 | 动态 bank/优先级位、无依据 Unknown、不写探测 |
| [ ] | B04 | SysTick 与读副作用 | CTRL/LOAD/VAL/CALIB、手动读与自动零 I/O |
| [ ] | B05 | M MPU 与 region 读取 | TYPE/CTRL/RNR/RBAR/RASR、容量及保存恢复 |
| [ ] | B06 | Debug、DWT/FPB 身份容量 | 不轮询 DHCSR、不自动使能、门控原因 |
| [ ] | B07 | M4 FPU 配置与身份 | CPACR/FPCCR/FPCAR/FPDSCR/MVFR、GDB regfile 复用 |
| [ ] | B08 | M7 cache/TCM 配置 | TRM/CMSIS 有据定义、维护命令不执行 |
| [ ] | B09 | M ID 探测与动态实例 | CPUID/ICTR/MPU_TYPE 等成功/缺失/非法结果 |
| [ ] | B10 | CorePrivate 和异构双核隔离 | 同 PPB 地址经不同核路由、缓存与失败隔离 |
| [ ] | B11 | 按 reader 运行态读取 | 安全 MMIO 正向、sysreg/GDB NeedHalt 拒绝 |
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

## 迭代和发布约束

每轮先专项验证；阶段边界和发布前完整回归。无新增改动或疑点不重复相同全量测试，不重建未改动的 OpenOCD。每轮形成可审查改动后提交并推送开发分支。只读允许必须恢复的 scratch/selector 临时写入，不自动使能或改变模式、权限。全部 36 项验收后，更新版本并按仓库发布流程发布非主分支 Release，注明仅软件验证及待执行硬件用例；完成后结束目标。
