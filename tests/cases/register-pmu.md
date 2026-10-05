# PMU 延后环境用例

REG-404：以下十项均未执行上板，状态 SKIPPED。软件 C 模型、真实 Tcl/MI 与固件离线编译另行记录，不能填充本表为通过。

准备实际芯片、core0/core1 的独立 target/服务映射、确切构建散列、EL2 Debug 授权与专用固件。保持暂停；不从调试器改变模式、开启计数、清零计数/溢出标志、解锁或修改过滤器。环境没有对应条件时继续 SKIPPED 并写原因。ADS 只作为同核/同停止点的独立对照，不导入资源，不把不同时间的计数当成相等。

| Case | 操作与验收 | 状态 |
| --- | --- | --- |
| PMU-H01 | 初始 E=0/1 分别读取 PMCR；EL2 新鲜证据确认四项事件容量，不把周期算入 N、不以 E=0 判不存在；前后全部控制一致 | SKIPPED |
| PMU-H02 | 固件独立预置非零周期高字，PMCR.LC=0/1 两种条件分别读取；一次 MRRC 返回完整 64 位，与固件基线一致，不发 32 位拼接/写指令 | SKIPPED |
| PMU-H03 | 四项 PMEVCNTR/PMEVTYPER 直接读取，与独立固件/ADS 静态基线对照；PMSELR 原值保留，未开始/停止/复位计数或修改过滤器 | SKIPPED |
| PMU-H04 | 选中 0–3 与 31；31 下 PMXEVTYPER 是周期过滤器，PMXEVCNTR 在数据指令前拒绝；4–30 等未实现事件也先拒绝，不切换选择器 | SKIPPED |
| PMU-H05 | 停止前 USR/SVC/Hyp 与当前 EDSCR.EL/HDD 独立记录；当前 EL2 允许，低 EL 保持 Unknown 且没有 PMU opcode；HDD 已知禁止不冒充未实现，不改变模式 | SKIPPED |
| PMU-H06 | 缺失/旧协议、错误 D16 身份、未知 PerfMon、矛盾 N/位宽或截短回应；不发布数据、无普通 MRC/MRRC/GDB 回退；已确定未实现项目无访问 | SKIPPED |
| PMU-H07 | 注入传输失败或 PC/状态/scratch/控制证据不一致；停止后续请求、FAULT/UNKNOWN 与服务隔离，无重试/盲目恢复；仅显式重连重新建立能力 | SKIPPED |
| PMU-H08 | core0/core1 不同数值与选中索引，Scope Core/All 分别读；只访问选定 owner，peer 计数控制及选择器不变，旧 core/context 请求拒绝 | SKIPPED |
| PMU-H09 | 请求开始后取消、实际线程/frame 变化和下一次停止/重连；完成已有原子恢复但丢弃采样，不重发；旧值保留旧证据，能力缓存正确失效 | SKIPPED |
| PMU-H10 | 使用专用只读固件 hook 与 JSON 驱动；全部 23 项原始数据匹配独立 snapshot，ready=0/已启用/基线不一致时拒绝；断开、配置、own/peer 控制均保留 | SKIPPED |

可执行入口：`node scripts/test-register-pmu-hardware.cjs` 默认五项 SKIPPED、无 target I/O。条件齐备后使用 `--run --binary EXE --project TOML --core CORE --case JSON`。单核不配置 `control_scope`；多核可在 JSON 明确配置 `control_scope: "all"` 与 `peer_core`，先确认两核都在该停止 hook。独立基线示例见 `tests/fixtures/register-pmu-board.example.json`，需要按选定核的实际 snapshot 更换所有 reference/expected。

该驱动的固定值对照要求 PMCR.E=0 且 HDCR.HPME=0，读者不会替你关计数。运行计数、溢出、Debug 冻结行为另按 H01/H02 手工记录有界变化，不能把冻结判成后端失败。没有合法高位/回绕设置条件时高位场景保持 SKIPPED；不要由调试器写计数器制造通过。
