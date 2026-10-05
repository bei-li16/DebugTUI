# 可选寄存器条件与判定依据自检（REG-110）

本批验收通用条件引擎、读取调度、保存和只读详情，属于开发分支的软件证据。目录 CPU、配置声明和软件夹具不能证明物理硬件身份或后端真实指令执行；具体类别的适配继续由 REG-001 至 REG-008、REG-301 至 REG-408 跟踪。

## 实现条件与读取结果

`Implementation::Yes/No/Unknown` 表示本次目录条件的判定，与 Sample.state/reason 的访问及读取结果分别保存。所有条件满足才为 Yes；有明确不满足项为 No；缺少能力且没有排除项为 Unknown。已知零与缺失事实分别保存，数量的 min/max 边界包含端点。无条件目录项保持 Unknown 身份，但可以沿用既有安全读取；有条件 Unknown 不自动读取，显式手动尝试仍需后端独立核验权限。

别名继承整条父链的条件、WO 和读副作用策略。父条件 Unknown 时别名不自动请求；父条件 No 时手动也不请求；父链有 WO 时任何读意图均不请求。读副作用仅可由明确的 manual 意图尝试。拒绝项不伪造实际值路由。别名共用父值和父请求，不增加重复读取。

读取权限不足、FPU 未使能、reader unsupported 和后端错误不转换为 Hardware not implemented。No 也只表示已保存的目录条件排除；配置声明导致的 No 不能冒充观察到的物理缺失。条件证据展示其来源及当前有效上下文，让用户区分配置结论和目标观察。

## 保存与显示

每个新值尝试的 `Sample.eligibility` 保存目录 CPU/架构/来源、worker context、alias 依赖、全部条件及范围、有效值、原配置值、来源类型和观察事实。只有 session、stop、core、frame 全部相同且物理 frame 0 的 Probe 可用于条件；有效观察优先于声明，声明仍独立保留。Probe 附带实际 MIDR 身份、thread、原始观察、时间、读取原因及解码说明。失败观察的旧字节不成为本次原始能力证据。

成功读取可以清除错误 detail，但不清除 eligibility。失败刷新保留原值时，`last_value_eligibility` 单独记录原值当时的依据：`kind=known` 附 Evidence；旧生产者没有证据则明确 `kind=unknown`。连续失败、共享 owner 拒绝和后续快照不能用最新失败或已丢弃样本的依据覆盖原值依据。旧 JSON 可反序列化；既有平面 `Snapshot.registers` 格式保持兼容。

System Regs 的 Status 选中条目显示 Current condition evaluation、Latest attempt condition evaluation 和 Retained raw value condition evaluation，按各自上下文标注。完整来源、条件、原始观察和未知原因可滚动查看；查看详情不提交 Probe 或值读取。显示沿用 ADS 风格的实现状态、访问原因、目录与值来源分开解释，不把 ADS 专有后端行为当作 DebugTUI 已具备的能力。

## R52 数量字段的核对

核对用户提供的 Cortex-R52 TRM `100026_0104_01_en`（718 页）：PMCR 13.3.1、表 13-2（第 435–436 页）规定物理 N=4，E 为独立使能位；ICC_CTLR 表 10-94（第 350 页）规定 PRIbits 编码 0b100，即五个物理优先级位。缓存的完整 Armv8-R AArch32 supplement `DDI0568A.c ID110520` 第 143 页说明 EL0/EL1 读取 PMCR.N 返回 HDCR.HPMN。用户另一份 26 页 `DEN0130_0100_en` 是 R-Profile 介绍，不能标为完整架构 supplement。

因此 `pmu.pmcr_n` 保留本次原始字段，仅在已适配 R52 身份、原生 PMU 事务的新鲜当前 Debug EL2 证明、匹配的响应/来源/上下文和 N=4 时发布 `pmu.counters`（本轮自检纠正了仅凭停止前 Hyp 模式确认数量的旧规则）。Guest/SVC 的 0/1/4 等值，或 Hyp 的未适配数量，不能证明物理缺失；无可用配置声明时数量条件保持 Unknown。PMCR.E 不参与物理数量判定，不启动或清除计数器。

`icc.ctlr_pribits` 与 `ich.vtr_pribits/prebits/listregs` 只保存原始字段。物理 ICC 和虚拟 ICV/ICH 容量分别要求当前停止 context 中原生 CTLR/VTR 响应的 matching MIDR、PhysicalCore、接口、MRC32 和完整时间区间，不从停止前 Hyp 推断。R52 适配值是物理/虚拟五位与四列表，六/七位矛盾值保持未知；ICH_VTR 不能替代物理 ICC AP 容量，详见 [GIC 自检](register-gic.md)。条件引擎的 5/6/7 位、数量上限测试是通用软件边界，不声明实际 R52 有六/七位物理接口；R52+ 和其他 CPU 的适配仍待各自手册与后端证据。

## 软件证据

| 要求 | 验证 |
| --- | --- |
| 全部内置可选条目及数量边界 | `all_builtin_optional_conditions_preserve_unknown_and_exact_count_boundaries` 遍历 M4/R52/R52+，验证缺失、min/max 和越界；这是模型测试，不证明对应 CPU 物理实现 |
| 已知零、缺失、全部条件、JSON | `capability_evidence_keeps_zero_missing_exclusion_and_all_declared_conditions` |
| 当前观察覆盖声明并保存原始来源 | `observed_capability_basis_records_raw_sources_and_rejects_other_stop_core_frame_or_session`；GIC 批次补齐 native 容量证明、请求区间、JSON/旧格式与保留值依据 |
| Probe 失败保持 Unknown | `capability_probe_failure_keeps_unknown_and_does_not_publish_retained_raw_as_current_basis` |
| WO/副作用和 alias 父链 | `aliases_inherit_write_only_and_read_effect_policy_without_overriding_manual_intent`；实际 worker 的 `write_only_alias_dependencies_never_issue_value_reads_even_with_manual_intent` 检查没有值请求 |
| 自动 Unknown 与显式手动 | `alias_parent_unknown_conditions_block_automatic_reads_but_keep_explicit_manual_reads`；旧实现失败日志 `artifacts/register-eligibility-before.log` 保留，不能计为通过 |
| 成功、Probe 过期、旧值依据 | `register_eligibility_sources_survive_success_expiry_and_retained_values` 通过实际 MI 验证 MPU observed 24 覆盖 declared 16，下一 stop 声明排除时不再读，旧字节与原依据保留；原 GDB/Hyp GIC 五位推断已移除 |
| 连续失败与旧生产者 | `retained_condition_basis_keeps_original_evidence_across_repeated_failures_and_old_snapshots` |
| 双核及 Scope All | `multicore_probe_uses_worker_generation_and_reads_one_owner_under_scope_all` 检查 core0/core1 的独立声明/观察和实际请求数量，不读取 peer |
| owner 变化后不复活已拒绝依据 | `shared_failures_keep_last_values_and_current_failure_lifetime_through_later_snapshots` 核对中间失败及后续 frame 快照，保持最后认可的 raw、provenance 和 eligibility |
| PMCR 的模式及虚拟数量 | `pmcr_guest_counts_and_unadapted_hyp_values_never_prove_physical_counter_absence`；`non_hyp_probe_never_reads_el2_or_attributes_virtual_icc_as_physical` 验证 SVC 的 N=0 保持物理数量 Unknown 且不自动读依赖项 |
| R52 物理 GIC 与虚拟接口分离 | `raw_gic_counts_and_stopped_hyp_never_prove_physical_or_virtual_ap_capacity` 与 `gic_capacities_require_independent_current_native_interface_evidence` 替代旧停止前 Hyp/虚拟数量夹具；真实 GIC worker 验证 No 条目的完整物理/虚拟依据及零数据访问 |
| 只读详情及窄宽滚动 | `register_status_preserves_current_latest_and_retained_condition_sources_without_io` 在 45×12、80×24、120×36 逐页核对所有来源，旧样本与请求数不变 |
| 测试助手退出响应完整性 | `headless_fixture_session_drains_quit_response_before_rejecting_exit_and_keeps_failures` 纳入 Cargo；四项 Node 流事件夹具验证 exit 早于末尾响应、缺失响应、错误响应及非零退出，保持失败分类 |

首次完整回归 `artifacts/functional-1791194988226-c34ab082/report.json` 在 VFP 驱动 CLEANUP 失败：读取、能力依据、数据视图和控制保持四阶段通过，正常退出仍被测试助手提前拒绝。原始失败日志和确定性复现 `artifacts/register-eligibility-session-before.log` 保留。开发测试助手原先在进程 exit 通知时拒绝 pending，请求管道可能尚有数据；现在在 close 通知后处理剩余 pending，先排空响应并保留证据文件。这不是把零退出一律计为通过：没有 quit 响应或响应错误仍失败，非零退出也失败。修复后的四项确定性记录为 `artifacts/register-eligibility-session.log`；助手不随安装包交付。

最终完整回归及严格 Clippy 见 [开发进度](registers-development-status.md)。[八项环境 case](../tests/cases/register-eligibility.md) 均 SKIPPED，未执行板级读取；REG-208 的实际终端视觉和完整目标类别的能力核对仍未完成。本批不修改 CPU 模式、FPU 权限、计数器、选择器后端协议或系统全局安装。
