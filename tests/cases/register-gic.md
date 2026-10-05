# GIC 延后环境用例

REG-408 与 REG-405 GIC 子集；以下十项全部 SKIPPED，未上板。软件模型、原生构建和离线固件编码验证不能计为芯片通过。

以 R52 TRM 100026_0104_01_en 表 10-69、10-71、10-80、10-94 和 §10.3.5 为依据。先准备实际 CPU/修订身份、每核明确 target/endpoint、确切二进制散列、暂停的独立 EL2 固件基线及静态中断环境。ADS 只作同核同停止点的独立原始值对照，不导入其资源。任何条件缺失保留 SKIPPED 并记录原因。

| Case | 操作与验收 | 状态 |
| --- | --- | --- |
| GIC-H01 | 核对当前 EDSCR.EL/HDD 与停止前 DSPSR；停止前 USR/SVC/Hyp 分别测试。EL2 原生证据明确物理 ICC/Hyp ICH；不通过改模式建立权限 | SKIPPED |
| GIC-H02 | 物理 ICC_CTLR 与虚拟 ICH_VTR 各有独立能力来源；R52 均五位、四列表。5/6/7 位架构夹具独立过滤；实际 R52 六/七位矛盾回应拒绝，不假装支持另一 CPU | SKIPPED |
| GIC-H03 | ICC_AP0R0/AP1R0 和 ICH_AP0R0/AP1R0 与独立固件/ADS 对照；ICV AP 显式派生同一 ICH backing 采样，不将 EL2 ICC 冒充 Guest ICV | SKIPPED |
| GIC-H04 | 已确认不实现的 ICC/ICH/ICV AP 索引 1–3，自动和手工均无数据指令；未知能力不自动试读，说明架构目录与实际容量区别 | SKIPPED |
| GIC-H05 | IAR0/IAR1 自动无请求，观测后端手工也拒绝；EOIR/DIR/SGI 等 WO 无读请求。pending/active、优先级、group enable 与控制前后不变，不 acknowledge/enable/deactivate | SKIPPED |
| GIC-H06 | 低 EL 下保留 Unknown，未执行 GIC opcode；记录 HCR.IMO/FMO 和 HSTR/ICH_HCR 的重定向/陷阱边界，不关闭陷阱或使能 SRE | SKIPPED |
| GIC-H07 | 29 项原生 MRC32、LR/LRC 分开，原始位宽/值与固件独立 snapshot 一致；不拼装 64 位，不假设跨条目/跨核原子性 | SKIPPED |
| GIC-H08 | core0/core1 不同值，Core/All 分别读取；只访问选定 owner，peer 控制不变；旧 core/session/generation 请求拒绝，配置文件不变 | SKIPPED |
| GIC-H09 | 旧协议、错误身份、截断/伪造容量或接口拒绝，无 GDB/普通 MRC 回退；传输/暂存/状态/PC/控制变化即 FAULT/UNKNOWN，隔离服务并停止后续访问 | SKIPPED |
| GIC-H10 | 取消、真实线程/frame 变化、下一停止/重连使缓存失效；完成在途恢复后丢弃采样。错误后旧值保留原始接口/owner/时间；未就绪或基线不匹配时驱动拒绝并断开 | SKIPPED |

可执行入口：`node scripts/test-register-gic-hardware.cjs` 默认五项 SKIPPED、无目标 I/O。环境齐备后：`node scripts/test-register-gic-hardware.cjs --run --binary EXE --project TOML --core CORE --case JSON`。使用 `tests/fixtures/register-gic-board.c` 在正常 EL2 执行捕获，每核独立 slot；在 `debugtui_gic_fixture_stop` 设置已有调试环境的停止点。固件和读者均不制造中断状态或切换模式。

JSON 示例 `tests/fixtures/register-gic-board.example.json` 是软件数据；物理执行前替换所有 reference/expected 与实际 MIDR，去除 software_example，保留独立 evidence_source。双核可声明 control_scope=all、peer_core 和 peer_ready，并先确保两核都暂停在 hook。默认单核不发送 select_core/default 或 control_scope 请求。

HPPIR/AP/LR 等状态可能受外部中断或 Guest 调度影响。只有独立建立静态环境才比较固定值；若无法稳定，不替操作者禁用中断、清除 pending 或 acknowledge，应保留相应用例 SKIPPED 并记录观察区间。物理 MMIO Distributor/Redistributor、完整 EL1 ICV 与 Debug 模块仍属于未完成 REG-405。
