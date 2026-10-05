# Setup 寄存器目录选择自检（REG-108）

本批验收 Setup 的 CPU preset、手工文件浏览、只读预览、取消、保存及配置／当前目标不匹配提示。延续 Arm Development Studio 的目录元数据与实际目标能力分离原则；目录名称、架构、分组、访问条件与能力 fact 范围来自工程 TOML，不把 ADS 安装资源作为运行或发行依赖。本批没有改变 ARM 指令、CPU 模式或 FPU 控制，也没有新增 R52+ 身份编码推断。

## 操作与来源

Setup 的 CPU registers 提供 Automatic、GDB target description、cortex-m4、cortex-r52、cortex-r52+ 和排序后的用户 preset。CPU 选择只修改项目草稿中的 `registers.cpu` 与 `registers.catalogue`，保留事实、reader/writer opt-in、Chip、核心、backend、启动及下载设置。Automatic 删除两个项目选择字段以恢复 profile/backend/chip 默认；GDB 同时显式清空两个字段。Save config / Ctrl+S 才保存项目引用，客户 profile、设备与目录文件不会改写。

CPU 选择器中的 F2 现在打开寄存器目录浏览，修复原来跳到 Project 浏览的问题。Register catalogue 的 Enter 可编辑路径，F2 可浏览 TOML。项目文件路径相对项目目录保存；继承路径保留其 profile 来源。选错／损坏的文件报错并保留原草稿及浏览器，供用户取消或重新选择。用户 preset 按 `<id>.toml` 查找；区分大小写的平台只列出可由这个规则解析的小写 `.toml` 扩展名，文件浏览仍支持显式路径。

选中 CPU、目录、候选 preset 或浏览器文件时，F1 打开可滚动预览。输入路径未提交时也能预览其错误。内容包括 Source、目录 CPU、架构、配置 CPU、芯片关联、每核 Observed CPU、目录规模、说明、访问条件与 capability fact 的 min/max。宽窄窗口和中文长说明均可逐行／翻页／Home／End／鼠标滚轮访问，Close／Esc 返回原选择器、浏览器或编辑框。弹窗中的 Start、保存快捷键与背景鼠标动作均不应用草稿或启动会话；Ctrl+Q 保持退出功能。

Source 沿用 REG-102 的 `file:`、`user:`、`builtin:` 和原 GDB 列表语义。有用户 override 却损坏时显示真实错误，不偷偷选择内置目录；一个 preset 的名字也可以是用户别名，Configured CPU 与目录 CPU 因此单独列出。

## 不匹配提示与多核身份

- 配置 CPU 与真正加载的目录 CPU 不同：提示两个值及文件优先规则。该提示不把用户 preset 别名判成硬件错误，也不拒绝用户有意选择的文件。
- 用户／内置 Chip CPU 关联与加载目录不同：提示配置不一致，明确标为 configuration only。关联解析与项目默认注入共享同一函数，用户值优先，空值沿用已有内置关联；没有关联不作猜测。
- 已有当前 Probe 的 adapted 身份与目录不同：Setup F1 和 Registers status 显示对应核心的警告。只使用当前 STOPPED、session、stop generation、core、frame 全部相同的 Probe；打开详情不会提交 Probe 或任何读请求。
- Setup 的目标设置已改动：原会话身份不用于新草稿。Chip、核心、endpoint、target、GDB、service、启动配置，以及寄存器 TCL endpoint／target／命令和 live-watch 路由参与目标比较；只改变 CPU／目录时可以继续核对原目标。运行、切核、复位／重连后的旧身份与其他核心身份均不能复用。没有当前核心证据时显示 Unknown，独立列出所有配置核心，不借 core.0 推断 core.2。

现有实际 MIDR 解码只适配 Cortex-R52 D13。选择 cortex-r52+ 的目录不会令 R52 或未适配 MIDR 变成 R52+，不会令可选扩展变成已实现，也不会使能 reader/writer。R52+ 实际身份和专有差异仍按 REG-001／REG-008 及后续类别验收。

## 软件证据

| 要求 | 生产路径与测试 |
| --- | --- |
| M4／R52／R52+、用户 preset、Automatic／GDB | `setup_cpu_catalogue_choices_preview_save_cancel_and_multicore_are_isolated`：独立子测试进程及 config root，真实 Setup 键盘／项目合并／目录加载／Ctrl+S／重新加载；继承文件、项目 CPU、用户目录及错误无回退 |
| 保留客户配置、多核及其他设置 | 同一矩阵核对非连续 core.0/core.2 及独立 endpoint、build/download、facts、非 register 原始设置；原 profile、devices 与用户目录逐字节保持 |
| CPU picker 浏览、文件及编辑取消、错误草稿 | 同一矩阵核对 F2 的 TOML 浏览、候选文件 F1、Esc、中文／空格相对引用、损坏文件保留浏览器、缺失输入保留编辑框及原文件；详情内保存／F5 不生效 |
| 用户发现与平台路径规则 | `setup_cpu_picker_discovers_sorted_user_presets_without_loading_or_rewriting_them`：排序、内置同名去重、无效 ID／扩展名／目录排除、损坏文件可见、平台扩展名规则、缺少用户目录及客户字节保持 |
| Chip 默认与警告不是身份 | `setup_catalogue_chip_association_preview_matches_default_resolution_without_identity_claims`：用户、空值内置回退、未关联型号及 Unknown；共用 `devices::Catalogue::cpu_association` |
| 当前证据与各核心隔离 | `setup_catalogue_observed_identity_is_current_target_and_core_specific`：core.0/core.2、运行、session／stop／core／frame，11 类目标／工具／路由变化，CPU／目录变化保留同一目标；匹配、不匹配、未适配 MIDR 和 R52/R52+ 区别 |
| 可滚动的完整说明与条件 | `setup_catalogue_details_scroll_all_conditions_and_unicode_at_supported_sizes_without_mutation`：45×12、80×24、120×36，所有页面及中文长说明、访问条件、Unknown、Home、滚轮、Close；无草稿／文件变更 |
| 实际 App 渲染接入 | `setup_render_imports_current_identity_and_clears_it_on_run_or_draft_target_change_without_io`：从 App snapshot 到 Setup 的当前身份，运行／换目标变 Unknown；请求队列为空 |
| Registers status 的当前身份警告 | `register_status_warns_on_current_observed_cpu_mismatch_without_probing_or_cross_core_reuse`：匹配当前 R52／M4 的警告、运行和五类失效条件、不适配身份、请求队列为空 |

预览遵守已有 REG-102 选择优先级与 REG-106 生命周期。前序配置、共享 owner、来源及取消测试在完整回归中继续执行；本批不以局部测试代替它们的验收。最终完整回归与 F24 记录见 [开发进度](registers-development-status.md)。[十二项环境 case](../tests/cases/register-setup-catalogues.md) 均 SKIPPED；TestBackend 不是实际 PowerShell／VS Code 视觉证据，REG-208 与最终 Release 仍待完成。此后 [REG-107 自检](register-distribution.md) 已核对目录打包／初始化／隔离替换与客户文件保留；最终升版后的真实版本升级、完整工具集和公网交付仍需后续验收。
