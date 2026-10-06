# 只读寄存器界面、状态与生命周期验收

本说明对应冻结 Goal 的 D01～D03。功能和最终通过记录只维护在 [唯一账本](registers-readonly-goal.md)；原 TODO 的长期扩展和写入阶段不因这次软件验收自动完成。没有执行上板测试，实际 PowerShell/VS Code 终端与探针组合仍按延后 case 验证。

三个核心参考文档保持绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

## ADS 参考与现有界面

逐张核对原 TODO 中用户提供的五张 ADS 截图，保留 `docs/images/registers/` 原图。没有使用 ADS 安装或分发其专有目录，也没有将截图中的定义数量作为本版支持数量。

| ADS 参考 | DebugTUI 对应行为与软件验证 |
| --- | --- |
| `ads-register-groups.png` | Core/SIMD/System、All 分类、Name/Value/Size/Access；树的键鼠展开、选择、滚动及搜索由 `register_tree_keyboard_mouse_scroll_and_description_filters_only_save_preferences` 验证。Total 是目录定义，Shown 是展开后的寄存器行，Valid 才是当前有效值。 |
| `ads-core-cpsr-banked.png` | CPSR 字段与 M 模式枚举来自同一父样本，非连续 IT 位段按逻辑顺序提取；字段可覆盖访问属性。字段帮助、枚举、说明和进制操作零额外读取。完整低 EL Banked 仍为后续范围。 |
| `ads-simd-registers.png` | Q/D/S 分组、128/64/32 位、整数/浮点/向量格式及共用原始位；未知、缺失或受限值分别解释，不能补零。格式模型验证符号、负零、NaN payload、跨字和 lane 顺序，不授予 FP 访问权限。 |
| `ads-system-timer.png` | System 多层分组和混合位宽继续复用现有树；目录、实现、读取成功分别计数。本次不将完整 Timer/Virt 等长期扩展加入验收范围。 |
| `ads-gic-unavailable.png` | Reader unsupported、Not implemented、Unavailable、Error、Write only、Stale 分开显示；WO 和维护命令不作为普通读取。截图中的 AP 数量不决定实际能力。 |

35 列时优先显示名称和值，完整宽度/访问属性、原始值和说明通过详情与 Status 可达；50/80/120 列保留 Size/Access，长中文名称与 128 位值不挤出这些列。值菜单的鼠标区域只覆盖值。变化高亮只比较同 owner、会话、核、帧下的有效读取；字段仅比较自身位段。失败、首次读取、跨上下文及过期值不显示成新变化。

`ui::registers` 测试使用生产 Ratatui 绘制函数和 TestBackend，检查列位置、颜色、命中区域、滚动和键鼠请求。设置 `DEBUGTUI_UI_ARTIFACT_ROOT` 可以保存实际终端 cell JSON 与文本，本轮在 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\evidence\readonly-ui-20261006` 导出 16 组结果。对应 PNG 由这些 cell 的坐标、颜色及符号生成并目视复核；这是软件渲染证据，不是实板或真实终端截图。

## 失败原因与旧值

普通逐项响应保留 `Sample.reason`；selector 整批失败继续保留后端 `Selector read REASON: ...` 的明确分类。修复前该 UI 分支只识别旧 synchronization unsupported，其他原因全部变成 Unknown。现在 AccessRestricted、HardwareNotImplemented、ReaderUnsupported、FeatureDisabled、TransportError 和 Unknown 能保持原分类，传输失败计为 Error；无分类的旧消息仍是 Unknown，不根据任意引用文本猜测原因。

主行可显示最近非零原值，但灰显、附失败/过期状态并不计 Valid。Status 的 `Reason` 展示具体原因和完整错误，Unavailable 汇总可进一步区分权限受限、未使能及未知。后端不支持与实际未实现单独计数；Target 隐藏有当前证据的缺失，All 保留其定义及原因。

整批错误没有新的值来源证明：保留旧原始值、原时间、owner、上下文、source 和 `last_value_provenance`/条件依据，不把旧 Debug EL、DSPSR、容量或 route 标成新请求证明。正常取消丢弃新值，恢复不确定继续 FAULT/隔离，不被取消覆盖。UI 不排队重试。

新增 `native_selector_failures_keep_distinct_reasons_and_the_original_debug_proof` 从实际 bank action 发起请求，逐类失败检查非零旧值、时间和完整 native 证明，另核对未受影响的 Core 值、状态计数、灰色绘制和零重试。有效目录的 native min_el 拒绝使用同一明确原因前缀；其 worker 用例确认零目标数据 I/O。

## 生命周期与证据边界

| 边界 | 当前行为 | 主要生产入口证据 |
| --- | --- | --- |
| 切核、异构目录 | 每核 worker、owner、端点及样本分离，迟到值不进入另一核；切 M7/M4 同 PPB 不复用目录或值，Scope All 只读选中核 | UI `per_core_register_ui_switches_catalogue_facts_and_drops_other_model_values`、`m_multicore_ui_switch_rejects_late_same_ppb_value_from_the_other_builtin_model`；相关 worker 已在迭代 16 的完整 selector 集成复验，本轮多核路由策略未改 |
| 帧/线程变化 | SelectedFrame 及由它派生的 Alias 过期；同停止点 PhysicalCore 不因仅换帧失效。返回旧帧不复活旧帧值；实际读取中的线程/帧竞争停止整批 | `register_lifecycle` 的帧/线程与嵌套 Alias 用例、UI `register_frame_cache_never_resurrects_on_return_or_accepts_late_frame_responses`；native selector 取消/实际帧改变用例 |
| 运行/再停止 | 旧停止态样本过期；迟到响应不覆盖新停止点。RunningMemory 按自己的请求区间、runtime 与 owner 判断，暂停后需要新读 | `register_access` 的运行通知和按需读取；实际 AP 运行态已在迭代 16 的完整 selector 集成复验，本轮 UI scheduler 又通过，运行态读取策略未改 |
| 复位及部分错误 | 发送前撤销旧值/Probe/名称；共享 reset 先失效受影响 worker，只发送一次，发送后的失败不复活缓存 | `restart_errors_console_symbol_replacement_and_elf_reconnect_invalidate_old_samples`、`multicore_selected_caches_shared_reset_and_console_expire_every_affected_worker_once` |
| 重连、ELF、路由配置 | 新 worker session 清空旧样本，旧 session 请求在数据前拒绝；ELF/连接配置变更按现有 Setup/断开重连或重启 workspace 流程生效，不能沿用旧路线的证明 | 同一生命周期集成核对符号索引、ELF、旧 context；A03 的每核配置路由及 A04 的有效来源证据沿用 |
| cluster/chip owner | peer 活动改变相应 owner lifetime，仅失效受影响共享对象；私有核和其他 cluster 保持分离，迟到共享结果丢弃 | `register_shared` 的六项真实 Coordinator/MI 集成与 `shared_ui_keeps_each_route_ages_only_changed_owners_and_rejects_late_owner_values` |
| 取消、错误及原来源 | 在途限制保持至回复；完整事务恢复后丢弃取消数据；故障隔离和旧来源保留 | `register_cancel`、native selector 六项集成、`register_provenance` 的实际 MMIO 路由与失败原来源 |

上述集成使用真实 Engine/Coordinator、MI 管道、TCP/Tcl 或 DebugTUI EXE；夹具只提供协议、不同核数值、延迟及错误，不证明 ARM 指令执行、实际 AP 集成或硬件状态。

实际终端与硬件操作步骤沿用 [框架 case](../tests/cases/register-framework.md)、[状态/取消 case](../tests/cases/register-status-cancel.md)、[缓存生命周期 case](../tests/cases/register-cache-lifecycle.md)、[共享 owner case](../tests/cases/register-shared-owners.md)、[M 多核 case](../tests/cases/register-cortex-m-multicore.md) 和 [R52 selector case](../tests/cases/register-r52-selector-read.md)，均 SKIPPED。软件 case 的通过不升级 hardware verified。

D04 的空闲 CPU/内存和请求计数对比仍需记录；最终完整回归、安装升级及非主分支 Release 由 E03～E05 验收，不以本批专项结果替代。
