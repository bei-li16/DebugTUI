# 寄存器缓存生命周期自检（REG-106）

本批验收限于主机端生命周期与缓存正确性。目标是让原值、时间、来源仍可追溯，同时使过期结果不能恢复为当前有效值。依据 [开发 TODO](registers-development-todo.md)；实际上板未执行，对应 [人工 case](../tests/cases/register-cache-lifecycle.md) 均为 SKIPPED。

## 采样语义

`register_samples[].view` 明确区分 `selected_frame` 与 `physical_core`，不再根据来源字符串推断。旧 JSON 缺少该字段时，保守按选中栈帧处理。

- 普通 GDB reader 读取选中栈帧；没有显式 MRRC 后端时，64 位 CP15 目录条目沿用 GDB 名称，来源实际记录为 `gdb:<id>`，也遵循栈帧语义。
- CP15、专用 MRRC、银行、VFP、MMIO 等直接后端读取物理核心状态。专用能力探测虽然可用 GDB，但已经验证 frame 0，采样明确属于物理核心。
- 多层 alias 继承根 reader 的语义，来源记录完整派生链，例如 `alias:d1@32 <- alias:q0@64 <- gdb:q0`。一个批次内仍复用父原始值，字段不另行读取。
- 所有值仍绑定 session、worker generation、实际 owner 和核心；选中栈帧值另外要求核心及 frame 一致。切帧只使选中栈帧值过期，同一物理停止代次的直接后端值可保留。

本批仅验收 REG-106 的 worker 上下文；跨 cluster／共享 owner 的独立代次和逐采样路由隔离在后续 [REG-109 自检](register-shared-owners.md) 单独验收。REG-210 的 endpoint、target、通道及类型化 provenance 矩阵在后续 [读取来源自检](register-read-provenance.md) 验收。

## 生命周期边界

| 边界 | 实际行为 | 证据 |
|---|---|---|
| 切帧及返回原帧 | 选中栈帧值和其 alias 永久转为 Stale；返回原帧必须重新读取，不能复活旧值；同代次物理值保留 | `frame_changes_expire_gdb_mrrc_fallback_and_nested_aliases_but_keep_physical_samples`；`register_frame_cache_never_resurrects_on_return_or_accepts_late_frame_responses` |
| 实际 GDB 线程／帧变化 | 每批选中栈帧读取前后检查实际线程、level、frame address 及暂停状态；发现变化即丢弃整批，不覆盖此前原值或时间，并结束后续读取 | `actual_frame_or_thread_changes_discard_the_batch_without_overwriting_last_valid_values` 覆盖读取前换帧、读取中换帧／线程 |
| Continue／新停止点 | 运行状态和新的 worker generation 使旧值失效；运行通知竞争停止该批次 | 既有 `catalogue_refresh_is_on_demand_and_snapshots_retain_precise_stale_values`、`running_notification_during_a_read_discards_the_result_and_stops_the_batch` |
| 切核 | 每核 worker 保留自己的缓存；UI 仅使用选中核的有效上下文，不能接纳其他核迟到结果；Scope All 读取仍只请求当前核 | `multicore_selected_caches_shared_reset_and_console_expire_every_affected_worker_once`；`late_response_after_core_or_session_change_is_discarded` 的核心／会话／generation 三种变化 |
| Reset 成功或部分失败 | 发送复位命令前提升代次、清除名称缓存与 Probe、使旧值失效；共享复位先使全部受影响 worker 失效，再仅发送一次复位命令 | `restart_errors_console_symbol_replacement_and_elf_reconnect_invalidate_old_samples`；上述双核集成及协调器 `shared_reset_is_once_and_refreshes_every_connection` |
| 原始 Console | 任意命令可能改寄存器、帧、符号或共享硬件，因此发送前保守失效。双核清除全部本地缓存，但命令只发给当前核；报错不撤销失效 | 双核集成；`console_invalidates_all_local_caches_and_sends_only_one_selected_command` |
| 替换符号／ELF | Console `file` 后重取 GDB 名称索引；普通 `set_elf` 要求先断开连接。更换后连接创建新 session，旧样本／请求不可复用。多核仍通过 Setup 更换共享 ELF 并重启 workspace | `restart_errors_console_symbol_replacement_and_elf_reconnect_invalidate_old_samples` 核对 r0 名称索引从 0 改为 1、新 ELF 命令、样本清空及旧上下文拒绝 |
| Reconnect | 各 worker 建立独立新 session，清空旧样本；显式新请求可读取当前核，旧 session 请求在发送目标读取前拒绝 | 单核及双核生命周期集成；UI 迟到回复测试 |

UI 收到 Snapshot 时同步失效信息，并保留最近已知原值。过期值既不计为当前成功，也不会阻止新上下文的按需刷新；界面不把 Snapshot 中的新值提前导入，从而保留正常的前次值比较与变化高亮。Status 同时显示采样视图。

## 软件验证

`tests/register_lifecycle.rs` 启动实际 session／协调器线程与 Node GDB/MI 管道。夹具只提供两核不同原始值、不同帧、名称变化、延迟和发送后错误；断言独立检查原值、时间、owner、会话、请求数量和命令顺序。它不连接 OpenOCD 或硬件，fixture ELF 仅作为 MI 文件身份，不证明加载实际 ARM 符号。

2026-10-05：`cargo test --locked --test register_lifecycle` 的四项集成全部通过。完整 `node scripts/test-functional.cjs --only unit` 为 **331 单元＋117 集成通过，2 ignored**，F24 的 **60 个模式全部有通过记录**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791171328667-720b123c/report.json`](../artifacts/functional-1791171328667-720b123c/report.json)，完整 Cargo 日志为同目录 `unit.log`；严格 Clippy 日志为 `artifacts/register-lifecycle-clippy.log`。Clippy、格式及差异检查通过。本批只新增勾选 REG-106，总计 **已完成／未完成 14/57**。
