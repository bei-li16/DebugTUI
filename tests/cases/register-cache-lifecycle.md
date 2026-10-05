# 寄存器缓存生命周期人工 case

当前状态：**全部 SKIPPED，未连接实板**。本文件准备环境可用后的验证，软件证据见 [生命周期自检](../../docs/register-cache-lifecycle.md)。使用实际开发版 DebugTUI、与之匹配的后端、可独立暂停和读取的双核 R52，以及含至少两层调用栈的 ELF。保存版本／提交、工程、拓扑、每核 GDB endpoint／实际 target、CLI JSON、MI/TCL 日志及终端截图。

每项先独立记录初始原始值和实际物理核；以状态、owner、session、generation、frame、view、source 和 timestamp_ms 验收，不根据界面出现数值判定成功。未具备第二核、可用 MRRC 或非零栈帧时，单独记录该子项 SKIPPED。

| Case | 操作及可检查结果 | 状态 |
|---|---|---|
| CACHE-H01 两核隔离 | 暂停双核，固件使 r0 或另一可用寄存器在两核具有不同可独立核实的值。切 core0／core1 各手动 Read；owner 与原始值对应。Scope All 不增加 peer 读取。返回 core0 后只能显示 core0 样本 | SKIPPED |
| CACHE-H02 栈帧及别名 | 选择 frame 0，读取 Core、一个 GDB 128 位父值及其 D/S alias，再选 frame 1。选中栈帧值及 alias 标为 Stale；同停止代次直接 CP15／MMIO 值可保留。返回 frame 0 后旧帧值仍 Stale，重新 Read 才有效；别名等于父值相应位段 | SKIPPED |
| CACHE-H03 实际上下文竞争 | 用可控 GDB 管道延迟一次读取，读取中通过已授权的外部调试入口切线程或帧。该批次报上下文变化，后续项不再发送；旧原始值与旧时间保留为 Stale，不能把迟到值算作成功 | SKIPPED |
| CACHE-H04 Resume 与迟到结果 | 发起一批读取后 Continue，再停止。运行中无新隐式目标读取；此前回复不能进入新停止代次。再次 Read 获取当前停止点值，保留旧值但不计其为当前成功 | SKIPPED |
| CACHE-H05 共享复位 | 双核先分别采样，再用明确配置的共享 Reset。核对全部 affected worker 在一次物理复位发送前失效；Scope Core 也不能让 peer 旧值继续有效。复位后新读与独立后端读取一致 | SKIPPED |
| CACHE-H06 复位部分失败 | 在隔离的可恢复环境中，让复位已发送后返回错误。两核旧样本必须 Stale，原始值及时间保留；无自动重试。确认真实目标状态后按正常恢复流程处理，不能以旧显示值判断目标状态 | SKIPPED |
| CACHE-H07 Console 共享影响 | 双核先采样，通过选中核 Console 发送一条已明确授权的共享状态改变命令。该命令仅发送一次到当前核，但两核旧缓存均失效；普通报错亦不使旧值复活 | SKIPPED |
| CACHE-H08 更换 ELF 和名称索引 | 单核断开后更换另一个有效 ELF，再连接；多核从 Setup 更换共享 ELF 并重启 workspace。旧 session 请求被拒绝，无旧样本导入；如更换目标描述，核对名称列表空槽和实际索引重新加载 | SKIPPED |
| CACHE-H09 重连及切核迟到回复 | 各核发起读取后执行 Reconnect／切核。旧核心或旧 session 回复不得改变当前视图；重连后每核 session 均更新，显式读取使用当前 owner | SKIPPED |
| CACHE-H10 采样来源与显示 | 分别验证 GDB Core、无 MRRC 配置的 GDB 64 位 fallback、显式 MRRC、直接 CP15／MMIO、物理 frame-0 Probe 和多层 alias。JSON／Status 的 selected_frame／physical_core 与实际通道一致；来源链完整，切帧不依赖来源字符串前缀 | SKIPPED |

软件集成测试可通过 `cargo test --locked --test register_lifecycle` 运行，UI 迟到回复与返回原帧覆盖在 `ui::registers`。以上人工 case 不会由该命令执行。每项保存 PASS／FAIL／SKIPPED、原因与证据路径；当前没有实板结果。
