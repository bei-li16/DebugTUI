# 多核寄存器归属与缓存（REG-109）

本批核对单个 TODO 的元数据、显式拓扑和缓存隔离。依据 [开发 TODO](registers-development-todo.md)；软件测试不证明 THA6 或 R52 实板的实际连接、cluster 关系、共享 MMIO 地址和调试权限。对应 [八项人工 case](../tests/cases/register-shared-owners.md) 全部 SKIPPED。

## 显式拓扑与元数据

目录 `scope = "core"`、`"cluster"`、`"chip"` 分别表达 percore、percluster、perchip；省略时为 core。alias 必须与父值具有同样的 scope，字段沿用父样本。项目示例：

```toml
[registers.topology]
chip = "board-1"
[registers.topology.clusters]
core0 = "cluster-A"
core1 = "cluster-A"
core2 = "cluster-B"
```

核心名称必须对应项目声明，cluster 从此表取得，不按编号猜测。没有 core3 映射时，core3 的 cluster 条目显示未知归属／Unavailable，保留空值，不发送读请求。chip 使用显式 topology.chip，或沿用已有 debug.chip 配置声明；两者均缺失时不读取 chip 条目。`registers_list` 返回实际配置拓扑及 `topology_source = "configuration"`，这不是硬件身份探测。

空 core／cluster 身份、前后空白、控制字符、超过 256 字节的身份或超过 1024 项的映射被拒绝，避免未知归属被误当成一个空名称的共享 owner。相同 cluster ID 表示同一声明域；不同 ID 彼此隔离。

## 缓存规则

UI 按 `(owner, register ID, sample core)` 保存当前及前次样本；样本自身还绑定 worker session、generation、栈帧／物理视图及来源。相同共享 owner 不会覆盖另一采样核心的值；切回原核可使用该路由仍有效的样本。不同 worker 的 GDB 会话及连接路径独立，因此不按同一个 owner 就跨通道复用原始值。一个批次内的 alias／字段继续复用父物理读取。

协调器另外维护每个 cluster／chip owner 的单调代次：

- 任一成员的状态、停止 generation、连接 session 改变或 worker 退出时，同 cluster 与 chip 的共享样本失效；其他 cluster 及其他核心的私有样本不受该失效操作影响。
- 未映射核心的实际 cluster 未知，不能假定它在所有已知 cluster 外；其生命周期变化保守地使全部已声明 cluster 和 chip 域失效。
- 新的停止通知即使状态仍为 STOPPED，也发布共享失效，当前界面无需等待下一次读取。
- 请求超时被标记无响应时立即发布相关 owner 失效，不再依赖该 worker 返回后续 Snapshot；私有值与其他 cluster 保留。
- 批次发送时记录 owner 代次。peer 活动改变代次后，该批次的对应共享新值被丢弃，保留最近已接受原值和原时间；同批私有值及未改变的共享域可独立返回。
- 共享失败保持准确 Error／Unavailable／Unsupported 及最近已知值，不把失败变成零或成功。重连的新 session 不会从协调器记录恢复旧原始值。

多核 JSON 的 `registers_list`／`registers_read` 提供 `owner_generations`，Snapshot 提供 `register_owner_generations`，每个共享样本提供 `owner_generation`。缓存调用方须同时核对 worker 上下文、owner 和当前 owner 代次；Rust 使用 `Sample::applies_at`。旧样本缺少代次时，在提供 owner 代次的多核上下文中不能视为当前有效。单核／原有私有样本省略该字段，既有 `Snapshot.registers` 结构不变。

自定义目录中使用 GDB reader 的共享条目，其兼容 `Snapshot.registers` 投影在首个 owner 失效通知中同步标记错误，保留最近接受原值，不等待对应 worker 的后续回复，也不追加目标读取。

界面在 owner 变化时只清除相关条目的失败退避记录，使共享值可再次按需读取；其他私有失败的退避保留。Status 显示配置 Scope／Owner、实际 Sample core、采样视图和 Owner lifetime。按 `t` 可滚动查看完整信息。

## 核对证据

| 要求 | 证据 |
|---|---|
| 三种 scope、显式身份、未知 cluster、字符串／数量边界 | `shared_owner_topology_validates_explicit_identities_and_never_guesses_cluster_membership`；既有 `owner_requires_explicit_cluster_topology_and_isolates_clusters`、`group_and_alias_cycles_and_scope_changes_are_rejected` |
| 共享代次、旧 JSON、安全缺省、私有样本不随 peer 代次失效 | `shared_owner_generations_are_required_in_group_caches_and_do_not_age_private_samples` |
| 四个真实 worker、两个 cluster、未知 cluster 和缺失 chip、逐核 owner／原始字节、alias 一次读取、Scope All 不广播 | `four_core_cluster_chip_alias_and_unknown_owners_use_exact_routes_without_cross_cluster_values` |
| peer Run／Pause，只失效相关 cluster／chip；未知 producer 保守失效；重连和旧上下文拒绝 | `a_peer_run_expires_its_cluster_and_chip_but_not_another_cluster_or_private_samples` |
| 读取中 peer 运行，私有结果仍有效，共享新字节及时间不覆盖最近接受值 | `peer_activity_during_a_shared_read_discards_new_bytes_and_preserves_last_accepted_sample` |
| 失败状态和代次经后续 Snapshot 保留；新 session 首次失败不导入旧原值 | `shared_failures_keep_last_values_and_current_failure_lifetime_through_later_snapshots` |
| peer STOPPED 状态未变化，但新的停止点立即发布共享失效；首个通知中的兼容 GDB 投影同步失效并保留原值，不增加读取 | `a_peer_stop_with_unchanged_state_publishes_shared_invalidation_without_a_new_read` |
| worker 超时后相关 owner 代次立即发布；私有及其他 cluster 不受影响，原值和时间保留 | `worker_timeout_expires_only_affected_shared_owners_and_publishes_the_boundary` |
| UI 三条独立路由保留九个样本，正确返回原核；相关域过期、私有失败退避及同 worker 上下文的迟到 owner 值拒绝；100×24／35×12 中键盘查看 owner、采样核心、代次、来源与未知归属，不触发读取 | `shared_ui_keeps_each_route_ages_only_changed_owners_and_rejects_late_owner_values` |

`tests/register_shared.rs` 启动真实协调器与四条 Node GDB/MI 管道。独立 endpoint／地址字节表提供原始输入；夹具不生成 owner、代次或缓存状态。MMIO 原字节、字段预期值、实际命令数量、通知顺序、状态与 JSON 由测试独立断言。定时通知只在该软件夹具的显式环境变量启用，不连接探针。

完整回归还发现并修复既有 MPU 夹具的输入污染：Windows 复用了进程 ID，旧测试目录中的 CPSR 覆盖文件使故障注入前已处于 SVC，模式变化断言因而失败。selector 夹具改用唯一目录且拒绝复用既存目录；MPU case 显式初始化覆盖文件并断言从 Hyp 开始再切至 SVC。MPU 生产逻辑未改动。失败记录保留在 `artifacts/functional-1791175916813-69363838/unit.log`，最终回归结果见下方。

完整功能入口、Clippy 和验收总数记录在 [开发进度](registers-development-status.md)。本批的 REG-109 范围不包括完整 endpoint／target／通道 provenance；REG-210 后续由 [读取来源自检](register-read-provenance.md) 单独验收。shared writer 的全部验证和其他硬件类别继续按独立 TODO 跟踪。

2026-10-05 完整软件验证：`node scripts/test-functional.cjs --only unit` 为 **335 单元＋122 集成通过，2 ignored**；F24 的 **69 个模式均有通过证据**，其余 **22 个功能套件未选择**。报告为 [`artifacts/functional-1791176224477-eaabe71a/report.json`](../artifacts/functional-1791176224477-eaabe71a/report.json)，完整 Cargo 日志为同目录 `unit.log`；严格 Clippy 日志为 `artifacts/register-shared-clippy.log`。Clippy、格式及差异检查通过。本批仅新增勾选 REG-109，总计 **已完成／未完成 15/56**。人工 case 全部 SKIPPED，未上板、安装或发布。
