# 寄存器配置与芯片关联自检（REG-102）

本批核对 REG-102 的旧配置回退、未知字段报错、路径来源及目录选择优先级。配置选择不证明硬件身份、后端指令或 R52+ 专有差异；本批没有修改 ARM 指令、CPU 模式或 FPU 状态。该配置批次之后，Setup 的完整元数据预览、CPU/目标不匹配及当前核心身份提示已由 [REG-108 自检](register-setup-catalogues.md) 核对；目录载荷、初始化、隔离安装与客户文件保留已由 [REG-107 自检](register-distribution.md) 核对；最终升版后的真实版本升级与公网交付仍按 REG-505 跟踪。

## 优先级与来源

1. 显式 `--environment` 优先于项目 `tools.profile`，后者优先于 `tools.root/debug-env.toml`。
2. Tools/profile 的根配置先与选定 backend 的配置合并，再由项目同名字段覆盖。组件映射可逐字段覆盖，未覆盖的 base/channel/字节序继续继承；最终必需字段与类型仍严格校验。
3. 芯片 CPU 关联只作为默认值。项目或选定 profile/backend 已存在 `registers.cpu` 或 `registers.catalogue`（包括空字符串）时不注入芯片默认 CPU。没有显式选择时使用用户芯片关联；用户芯片未声明 CPU 时沿用内置芯片关联。
4. 合并后的非空 `registers.catalogue` 文件优先于 CPU preset。否则按 CPU 名查用户 `profiles/registers/<cpu>.toml`，没有该路径时使用同名内置目录。无 CPU/目录时保留原动态 GDB 列表。

项目的相对 catalogue 路径基于项目文件目录；profile/backend 的路径基于声明它的 profile 目录，`${profile_dir}` 同样以该 profile 为准。显式环境参数的相对路径由进程当前目录解释，后续继承的目录路径仍相对于被选环境。空格、中文、绝对路径和 `..` 保留各自定义来源，不随着切核改变。

只设置项目 CPU 不会自动删除 profile 继承的 catalogue 文件。如果要改用 CPU preset，同时写 `catalogue = ""`；如果要回到 GDB 列表，同时写 `cpu = ""` 与 `catalogue = ""`。Setup 选择 CPU/GDB 已在草稿中执行相应清除，Automatic 则删除项目选择，让 profile/chip 默认值重新生效。字段互相独立，不能把显示的配置 CPU 当作实际文件目录 CPU。

Source 分别使用 `file:<已解析路径>`、`user:<用户目录路径>`、`builtin:<cpu>` 或 `gdb`。文件不存在、损坏、过大、错误格式，或用户同名路径实际上是目录时均报错；有用户 override 却无法读取时不偷偷回退到内置模型。自检修复了原先 `is_file` 把非文件 override 当作不存在的问题。

Config、Topology 与 Component 的未知字段/错误类型由严格 schema 拒绝；后端命令与 topology/target/fact/channel 等名称继续由有效配置校验。项目、profile 根及选定 backend 的未知 register 设置均有实际程序错误证据。未选 backend 不等于实际目录选择，不用它证明当前 reader 能力。

## 软件证据

| 要求 | 证据 |
| --- | --- |
| 旧配置与完整新配置 | `register_configuration_schema_rejects_unknown_nested_fields_and_wrong_types`：无新字段时 GDB 回退；完整 reader/writer opt-in、事实、target、component、topology 往返；15 种错误 schema 输入 |
| project/profile/backend/chip 优先级 | `register_selection_priority_respects_project_profile_backend_and_explicit_empty_selectors`：九组矩阵，两个非连续物理核心名称，explicit empty、文件优先及不猜系统寄存器 target |
| 芯片关联与客户文件 | 既有 `cpu_association_priority_and_register_extensions_survive_upgrade`：用户/内置芯片关联、项目 CPU、空值与客户目录保持；不是 npm/EXE/ZIP 升级验收 |
| 路径与字段继承 | `register_configuration_paths_cover_tools_root_explicit_environment_and_project_overrides`：tools.root、显式环境覆盖缺失 tools.profile、profile_dir、中文/空格路径、project 文件及组件局部覆盖；既有 `register_catalogue_paths_follow_the_declaring_profile_or_project` 保留保存后 profile 原字节断言 |
| 错误不能被芯片默认掩盖 | `register_configuration_errors_survive_selected_backend_and_chip_defaults`：项目/profile/选定 backend 未知字段、标量 register table 及未知 CPU |
| 用户/内置/文件优先级 | `a_user_cpu_preset_overrides_builtins_and_invalid_overrides_do_not_fall_back`：客户内容、损坏文件、同名目录与真正不存在；`explicit_catalogue_precedes_cpu_and_missing_file_is_an_error`：显式文件优先及错误不得回退 |
| 实际 EXE/JSONL 与多核 | `actual_binary_register_configuration_priority_errors_and_multicore_identity_are_isolated` 执行 `scripts/test-register-configuration.cjs` 十项独立 case：旧项目、chip/profile/backend/project/file/empty/user 来源与错误矩阵；各 core.0/core.2 返回独立 session；客户文件 hash 保持，未执行 connect/MI/probe |

本批修复芯片默认覆盖 profile 显式 CPU 的优先级错误。有效选择先于芯片关联，使实际 `registers_list` 与 Setup Automatic 的“继承 profile/chip”语义一致。选择 R52+ 目录不等于已证明实际 R52+ 身份；未识别硬件仍遵守既有 Unknown/禁止扩展探测规则。

完整回归、F24 与严格 Clippy 的最终记录见 [开发进度](registers-development-status.md)。环境 case 见 [配置验收](../tests/cases/register-configuration.md)，均为 SKIPPED；实际终端视觉、各类交付安装升级和完整 CPU/后端能力矩阵仍未完成。

STM需要明确chip owner的控制区映射，不能从CPU型号或 `components` 无归属映射推断。可选 `stm_hwe`/`stm_dma` 只有同基址、通道和字节序的独立映射才探测；数据读取总是依赖当前Proof，`mmio_probe=false`不关闭此校验。完整示例、功能条件和范围见[STM](register-stm.md)。
