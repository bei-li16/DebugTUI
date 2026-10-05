# 寄存器基础框架逐项自检

本次核对以 [开发 TODO](registers-development-todo.md) 的单个条目为边界。软件目录、模型、GDB 管道及界面要求可以分别验收；它们不证明目标硬件支持所有目录定义，也不代表整个阶段或最终交付完成。按用户要求，本轮不执行上板测试，保留对应 [环境用例](../tests/cases/register-framework.md)。

## 本轮修正

- 目录读取在检查文件长度后还限制实际读取量，拒绝超过 4 MiB 的增长文件或输入流，以及非法 UTF-8。版本、数量、引用、循环、位宽和 reader 参数继续严格校验。
- 宽窗口使用固定的 Name、Value、Size、Access 列。名称及过长值按终端字符宽度省略，Size／Access 不随名称或 128 位值被挤出屏幕；值格式菜单的鼠标区域只覆盖值列。窄窗口保留名称与值，说明区使用当前字段的位宽和访问覆盖。
- 当前有效值与同一 owner、会话、核心和栈帧的前次有效值比较，以琥珀色显示变化；字段单独比较提取后的位值。首次读取、未变化、跨核、跨会话、跨栈帧和过期值不显示为当前变化。采样仍按 owner 保存，显示偏好仍按芯片、核心、CPU 和目录隔离。
- 字段 Status 新增父描述、字段位宽／访问属性、按逻辑低位优先排列的 bit segments、当前／最近已知字段原始值及完整枚举表。长说明、访问条件、读取原因、来源和完整 128 位原始值可通过键盘滚动访问。

## 条目与证明范围

| TODO | 核对的完整软件要求 | 主要证据 |
|---|---|---|
| REG-101 | 版本、未知字段、重复寄存器／组 ID、缺失组及父组、组和别名的间接循环、8/16/32/64/128 位限制、字段越界／重叠、非法／重复枚举、八种 reader 缺参、4 MiB 边界与增长读取 | `registers::tests::strict_catalogue_validation_and_bom`、`catalogue_entry_limits_and_indirect_cycles_reject_invalid_extensions`、`catalogue_file_and_stream_limits_cover_growth_utf8_and_complete_reader_parameters`、`group_and_alias_cycles_and_scope_changes_are_rejected`、`cp15_and_mrrc_encoding_widths_are_distinct`、`overlapping_and_out_of_bounds_fields_are_rejected` |
| REG-103 | 描述及位宽由目录提供；逐项原始值、状态、implementation、reason、原错误、owner、停止上下文、毫秒时间、来源保留；`Snapshot.registers` 的 name/value/changed/error 兼容形状不变 | 实际 worker／MI 的 `catalogue_refresh_is_on_demand_and_snapshots_retain_precise_stale_values` 和 `gdb_reader_returns_128_bits_and_derives_aliases_from_one_parent_sample`；停止后只改变有效性，保留原值、时间和错误 |
| REG-104 | 指定 ID 批次逐项读取，GDB 名称空槽保持实际索引，单项错误不丢弃其他 Core／扩展值；运行竞争结束后续读取 | `catalogue_refresh_is_on_demand_and_snapshots_retain_precise_stale_values` 核对 MI 索引 0/2/3 和中间 PC 错误；`running_notification_during_a_read_discards_the_result_and_stops_the_batch` 核对只发送首项 |
| REG-105 | 连续与非连续字段、枚举、多层别名、64/128 位高低边界和两种字节序 | `cpsr_discontiguous_it_and_mode_enums`、`asymmetric_bytes_and_cross_half_fields_preserve_bit_order_at_every_width`、`all_raw_widths_and_endianness_roundtrip_without_json_precision_loss`；128 位 GDB 父值派生 D/S 的集成测试核对仅一次物理读取；显示格式另由 `registers::display::tests::` 验证 |
| REG-201 | 保留 System Regs 入口；Core／SIMD／System 树及 Quad／Double／Single 子组；键鼠展开、选择及翻页／滚动 | `register_tree_keyboard_mouse_scroll_and_description_filters_only_save_preferences`、`keyboard_mouse_fields_and_search_cancel_do_not_issue_reads`；无目录的既有平面显示分支保留 |
| REG-202 | 四列可见；长客户名称、中文和 32/64/128 位值不遮挡 Size／Access；窄窗保留名称和值，说明区及 Status 可查看完整字段／寄存器信息 | `register_columns_keep_size_access_and_value_hits_visible_for_long_names_and_128_bits` 覆盖 35/50/80/120 列及鼠标区域；`register_field_columns_and_details_use_the_field_width_and_access_override`；`register_help_preserves_the_full_128_bit_raw_value_when_the_main_row_is_clipped` |
| REG-203 | CPSR 字段树及 M=19 的 AArch32_SVC；IT 的非连续段；父字段使用同一原始样本，展开／帮助不增加目标请求 | `cpsr_enum_and_stale_values_render_in_wide_and_narrow_views`、`cpsr_discontiguous_it_and_mode_enums`、`register_field_help_keeps_all_enums_conditions_bits_and_chinese_description_scrollable`；高亮测试只提交 CPSR 父请求，N/C 由返回值派生 |
| REG-204 | 名称／ID／说明搜索，取消或提交搜索；All／Core／SIMD／System 分类；操作不读取整目录 | `register_tree_keyboard_mouse_scroll_and_description_filters_only_save_preferences` 核对中文说明、全部分类及只提交偏好；`expanded_groups_and_committed_search_save_only_view_preferences`；`only_visible_expanded_registers_are_read_and_pending_does_not_accumulate` 验证后续按需读取仅限可见项 |
| REG-207 | 旧进制偏好迁移、逐核及字段变化高亮、展开／字段／筛选／格式偏好持久化，芯片／核心／目录之间不串用 | `register_view_preferences_migrate_once_and_restore_by_chip_core_and_catalogue`、`register_change_highlight_compares_same_owner_and_field_bits_only_after_valid_reads`、`register_float_vector_menu_uses_keyboard_mouse_and_never_reads_or_changes_samples`、`scoped_register_preferences_merge_concurrent_clients_and_preserve_root_ui`、`actual_binary_register_preferences_preserve_other_clients_and_never_start_gdb` |
| REG-209 | 寄存器及字段描述、所有枚举符号、访问条件、原始读取状态原因；键盘进入、滚动和退出；鼠标悬停为可选项 | `register_field_help_keeps_all_enums_conditions_bits_and_chinese_description_scrollable` 覆盖 35×12／80×24、CPSR.M 全部模式、IT 的 8 位及 bit segments、中文长说明、访问错误及采样不变；`register_status_details_keep_long_reader_reason_source_and_sample_time_accessible` |

测试直接运行生产解析、字段算法、App 键鼠／Ratatui 渲染及实际 session worker／MI 管道。GDB 软件夹具只提供原始目标响应，128 位和别名的预期值及实际请求次数由独立断言核对。UI 测试读取终端单元格时跳过宽字符的 continuation cell，防止把 TestBackend 遗留的占位单元格误当作中文内容。

## 验证记录

2026-10-05：`node scripts/test-functional.cjs --only unit` 通过，其中完整 `cargo test --locked` 为 **328 单元＋113 集成通过，2 ignored**。严格 `cargo clippy --locked --all-targets -- -D warnings` 通过。F24 的 **53 个证据模式全部有通过记录**，没有缺失模式。功能报告为 [`artifacts/functional-1791168282740-48d58bdf/report.json`](../artifacts/functional-1791168282740-48d58bdf/report.json)，完整 Cargo 日志为同目录 `unit.log`；Clippy 日志为 `artifacts/register-framework-clippy.log`。本轮只运行 unit suite，其余 **22 个功能套件未选择**，不计为已执行；2 个既有环境测试仍 ignored。

以上十项与此前 REG-205/206/211 合计 **已完成／未完成 13/58**。用户排除实际上板；硬件条目后续仍需核对驱动／人工 case 的完整性和跳过记录，不能仅因已有局部软件夹具就自动勾选。

## 仍未完成的验收

上述基础框架批次之后，REG-106 由 [缓存生命周期自检](register-cache-lifecycle.md)、REG-109 由 [共享归属自检](register-shared-owners.md)、REG-210 由 [读取来源自检](register-read-provenance.md) 单独验收，当前总计 **16/55**。REG-102、REG-107/108/110、REG-208 和后续阶段仍未完整验收。它们仍需配置／路径完整矩阵、npm／EXE／ZIP 升级交付、CPU 目标不匹配、可选类别的判定和 PowerShell／VS Code 视觉结果等相应证据。

R52+ 专有身份／差异、合法 EL1／Guest／User VFP、FP 状态控制 writer、系统／银行 writer、64 位 MMIO、GIC／Debug／STM 完整适配和最终安装／Release 仍待完成。当前 EL1 VFP 限制及架构依据继续见 [状态与取消](register-read-status-and-cancel.md)。本轮没有改动后端指令实现，没有使用 Hyp 软件测试代替其他模式支持。
