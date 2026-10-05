# 寄存器实际读取来源自检（REG-210）

本批为开发分支的软件验收。目录声明、CPU 选择和拓扑配置均不能证明物理硬件身份。R52 权限、调试态恢复和后端指令仍沿用前序依据 R52 TRM 与 Armv8-R AArch32 手册的实现；本批不更改指令编码、模式或 FPU 控制。

## 数据与界面约定

每个新生产的 Sample 附带 `provenance`。其中 `catalogue_reader` 是目录声明，`acquisition` 区分普通读取、能力探测、MPU 总览和选择器 bank；`access` 才是实际值请求的路由记录。未进入值请求的拒绝项保留 `access = null`，不能沿用前一项的请求。

`access` 保存实际命令、worker context、请求时间和以下路由之一：

| 类型 | 记录与限制 |
| --- | --- |
| `gdb_register` | 实际 GDB 名称和稀疏索引；DebugTUI 成功选择的 endpoint 与配置 endpoint 分开保存 |
| `gdb_memory` | 实际 MI 地址、读取位宽、配置字节序；普通字节读不保证原子性 |
| `tcl_register` | 使用的 TCL endpoint、请求选定的 target 和值操作；target 名称不证明物理 CPU 身份 |
| `tcl_memory` | 配置通道来源、channel、endpoint、请求 target、地址、值位宽、bus width/count、布局字节序和原子性 |

GDB 的已选 endpoint 只在 DebugTUI 自己成功执行 `-target-select` 后已知。任意用户 Console/opaque CLI、断连或重新选择会清除该证据；即使 Console 文本包含新的地址，也不解析它来猜测连接。内部固定的 `maintenance flush register-cache` 成功后保留原连接证据，该例外不适用于用户 Console。历史样本保留原来源，新样本显示未知与独立的配置地址。

阶段 `planned` 表示路由已解析但尚未进入发送，其时间为路由解析时间；`started` 在实际管道/socket 写入开始处记录，结果尚未确认；`responded` 表示匹配 MI 响应或完整 TCL 响应已经收到。服务锁、非法命令或连接失败不能冒充发送；收到错误或畸形完整响应不能冒充有效值，仍按 Sample.state/reason 判定。进入发送后的时间在实际写入处取样，而不是等待服务锁之前。

别名的 `aliases` 按根到子记录来源、偏移和位宽，沿用父值的完整 Access，包括原命令和时间。VFP 的重叠 S/D/Q 视图沿用首次物理 pair 请求的来源，后读 D1 不会把缓存中的 D0 请求改写成 D1。

失败刷新保留旧值时，最新失败尝试仍写在 `provenance`，旧值来源另存 `last_value_provenance = {status:"known", provenance:...}`。来源缺失的旧生产者使用显式 `status:"unknown"`，避免把新失败尝试当成旧值来源。连续失败、共享归属过滤和 UI 合并均保留这一区别。整条 selector 请求报错而未返回新的路由证据时，UI 不把旧来源标成最新尝试，旧原值仍独立保留来源。

Status 展示目录来源、目录 CPU/架构、配置 CPU 选择、具有当前停止上下文证据的 Observed CPU、Scope/Owner、Sample core、共享 owner lifetime，以及最新尝试和保留原值来源。窗口可滚动查看完整地址、target、位宽、通道和时间。请求 target 与观察身份分开；内部 Tcl 协议脚本留在 JSON 中，不进入产品弹窗。打开详情不触发读取或探测。

## 逐项软件证据

| 要求 | 实际测试证据 |
| --- | --- |
| GDB 与嵌套 alias 共用一个实际请求，Console 后不猜连接 | `gdb_reader_returns_128_bits_and_derives_aliases_from_one_parent_sample`：一条 MI 值请求、完整原位及根到子 derivation；opaque CLI 后新来源未知、旧来源保留 |
| CP15 和普通 Backend | `cp15_routes_physical_core_and_restores_target_in_one_packet`；`generic_backend_reader_records_its_target_request_without_claiming_banked_access`：实际 TCP 请求、正确 cpu0、独立 get_reg 与银行指令 |
| 64 位 CP15 的两种路由 | `mrrc_scope_all_reads_only_selected_core_with_full_width_and_rejects_old_context`：Scope All 只读 cpu1，实际 TCL MRRC；`frame_changes_expire_gdb_mrrc_fallback_and_nested_aliases_but_keep_physical_samples`：目录 cp15_64 但实际使用命名 GDB 寄存器 |
| Banked 与 VFP | `all_builtin_banks_use_exact_words_preserve_target_and_avoid_legacy_get_reg` 的 23 个银行请求；`every_vfp_storage_view_uses_shared_physical_pairs_with_exact_alias_bits` 的 86 个值视图、22 次实际请求及 pair 来源完全一致 |
| MMIO 显式 bus channel | `mmio_channel_metadata_matches_exact_bus_requests_and_preserves_old_origin_on_failed_refresh`：严格 TCP 夹具核对 32 位与两个 32 位组成的 64 位读、LE/BE、alias 不重复、错误响应和旧来源分离 |
| 默认 GDB MMIO 与多核共享域 | `four_core_cluster_chip_alias_and_unknown_owners_use_exact_routes_without_cross_cluster_values`：四条独立 MI 管道，逐行核对 endpoint、地址、context、别名父请求和 unknown owner 不发送 |
| Probe、MPU、选择器 | `explicit_probe_decodes_evidence_filters_ap_registers_and_expires_at_next_stop`；`direct_mpu_overview_reads_implemented_regions_and_current_mair_without_mcr`；`real_tcl_selector_transactions_read_pairs_and_restore_each_selector_and_target`：逐行 acquisition/路由/响应证据，禁止项没有串用上一项来源 |
| 不虚报发送或成功 | `rpc_progress_never_claims_dispatch_for_invalid_commands_or_a_service_fault`；`rpc_progress_records_dispatch_and_complete_frames_even_when_value_parsing_fails`：真实 socket 边界、无请求字节、断连与完整错误/畸形响应 |
| 兼容与连续失败 | `legacy_register_source_labels_never_infer_an_executed_route_or_connected_endpoint`；`retained_register_values_keep_the_last_valid_origin_across_failures_and_legacy_unknowns`：JSON 往返、unknown、旧来源不被最新失败覆盖 |
| ADS 风格详情、Scope/Owner 与 CPU 身份 | `provenance_popup_distinguishes_catalogue_cpu_scope_latest_attempt_and_retained_origin_without_io`：Core/Cluster/Chip、100×24 与 35×12 完整滚动、失败/旧值两条路由、实际 MIDR 解码、过期/运行时身份拒绝和无新增请求 |

完整 Cargo、功能入口 F24 和严格 Clippy 的最终报告见 [开发进度](registers-development-status.md)。前序缓存、owner、取消、写入和非寄存器测试一并回归；不以新增测试的通过代替完整回归。

前序 REG-109 组合自检发现：worker 保留的共享新字节在 owner 变化时已被协调器丢弃，但下一次失败刷新会把那些字节作为“旧值”放入中间快照。失败已由严格独立字节夹具复现，记录在 `artifacts/register-provenance-rejected-refresh.log`。协调器现在对未获认可的非 Valid 快照同样按该路由的最后已认可样本回填，清除没有保留值时的旧来源；新增断言覆盖失败的中间快照、Response 及后续 frame/status 快照，原始字节、来源和时间均不能复活。

软件证据满足 REG-210 的来源展示范围。所有环境 case 见 [读取来源验收 case](../tests/cases/register-read-provenance.md)，本轮均 SKIPPED；其结果不外推为实板采样成功或实际终端视觉验收。REG-001/102/107/108/110、REG-208、尚未适配类别和 writer、完整工具交付与最终 Release 按 TODO 继续跟踪。
