# 每核寄存器配置与声明来源

Setup 的寄存器详情、System Regs 的 Status 和 headless `registers_list.configuration` 显示每核最终配置值、声明文件/section、被覆盖的声明以及每核整表替换。它们解释配置，不证明目标型号、模块实现、当前 Debug EL 或后端访问权限。

## 配置层与规格的对应

| 规格中的职责 | 现有工程中的位置 | 显示的来源 |
| --- | --- | --- |
| 芯片拓扑及连接 | 项目、Tools/profile、devices 芯片关联、每核配置和 memory_access | 项目/环境的实际声明文件；芯片默认单独标为 association |
| 核寄存器定义 | CPU preset 或显式 catalogue，公共 extends 加型号增量 | 配置的选择来源、实际加载的目录来源及条目的继承文件分别展示 |
| 调试架构与后端能力 | 现有 GDB、OpenOCD reader、命令/通道及运行时能力证据 | 配置声明与实际采样 route/provenance 分开；不新增一套 TUI 指令注入流程 |

当前解析顺序是 profile 根、选定 backend、项目，再应用该核的 `cores.registers`。芯片 CPU 关联仅在项目及选定 profile/backend 都没有显式 CPU/catalogue 选择时提供默认。内置值是最后的缺省来源。显式 `--environment` 或 `--tools-dir` 所选择的真实环境文件替代项目的 Tools/profile，不把未选择的文件记成来源。

环境根、backend 和项目表递归合并，逐个 map key 保留来源；项目的空表不会清除环境中已有的 map。每核 `cores.registers` 的省略字段完整继承，显式字段替换；显式 map **整表替换**。每核空 `facts = {}` 清除本核继承 facts，空 `cpu`/`catalogue` 清除本核选择。完整 component 绑定仍须通过既有校验，不因诊断输出允许不完整路由。

`cores` 及每核 `cores.registers` 仅允许在项目中声明，环境根和 backend 中的 `cores` 在加载时明确拒绝。因此每核声明来源是项目文件，不将不支持的环境输入当作另一层有效来源。

例如环境给出 `facts.inherited=4` 和 `facts.capacity=1`，backend 将 capacity 覆盖为 2，项目覆盖为 3，则显示 capacity 的最终值 3 和前两项声明来源；inherited 仍来自环境根。某核设置 `facts={selected=7}` 后，该核仅保留 selected，并显示整表替换及移除的 inherited/capacity key；其他核保持各自有效配置。

## 输出和冲突解释

`configuration` 包含 `core`、`settings` 和 `replacements`。每项 setting 的 `path` 是分段数组，避免把 `facts["nvic.priority_bits"]` 与嵌套字段混淆；`value` 是当前有效配置，`source` 是声明来源，`overrides` 保留先前的值和来源。replacement 给出整表替换的核、字段、继承来源和被移除的 key。

CPU 配置来源与实际目录 CPU 分别显示。显式目录文件优先于 CPU preset，名称不匹配时 Setup/Status 解释冲突；芯片关联仍是配置默认。当前观测型号来自已有、仍适用的 Probe。仅修改每核 CPU/目录选择保留当前目标观测，以解释型号冲突；修改 memory_access 路由会使旧 Setup 身份观测失效，不能把原 AP/target 的型号用于新路由。

相对目录按声明文件解析；来源记录实际环境文件和 backend section。运行时/CLI 改动若与加载记录不一致，则新值标为 `runtime/inline configuration (origin unavailable)`，保留已知的旧声明供解释。来源数据只保存在内存，不写入客户 TOML，不增加持久化字段或目标 I/O，也不改变 CPU/目录的原有优先级。

Setup 显示当前草稿，保存前不会改写项目或共享 profile。Status 的配置详情可以用方向键/PageUp/PageDown 滚动查看；切核后对应当前核。配置的 memory channel 展示允许的核、target、endpoint 和 while_running，实际成功访问仍以样本 provenance 为准。

## 验证边界

软件覆盖 profile/backend/project/per-core 来源、相对路径、显式环境、芯片默认、覆盖链、空表/清除、非连续核编号、运行时来源失效及持久化兼容。Setup/Status 在窄/宽终端中逐页检查来源和切核，并断言无读取请求；实际 EXE 的隔离配置套件检查两个 worker 的 JSON、无 GDB 启动及客户文件字节不变。

测试可设置 `DEBUGTUI_TEST_ARTIFACT_ROOT` 指定产物目录；Rust 的 MI/Tcl 集成夹具、配置及 source-tabs 单元夹具和既有 Node 驱动使用该目录，未设置时仍使用工程 `artifacts`。`CARGO_TARGET_DIR` 仅指定构建位置，不能替代这个设置；源码、内置目录和 mock 程序仍从工程读取。空间不足的失败日志保留，修正产物位置后重跑，不将软件失败改为 SKIPPED。

延后环境 case 为 [CONFIG-H12/H13](../tests/cases/register-configuration.md)。未执行上板，状态 SKIPPED；软件检查不替代芯片、AP 或后端的实板验收。

核心参考文档（绝对路径保留）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
