# REG-210 读取来源环境 case

初始状态：以下 12 项均 **SKIPPED**。本批只有独立 MI/TCP 软件夹具与终端缓冲测试，未连接探针。执行人员逐项保存项目/profile、目录 SHA256、DebugTUI/OpenOCD/GDB 版本、核心与停止位置、原始 JSON/日志及 Status 截图，记录实际结果和差异。

前置条件：构建开发分支；使用已审核的环境配置与 ELF；逐核暂停于物理 frame 0，确认目录及权限适合实际 R52。CPU 配置、catalogue CPU、target 名称、端点和板上物理身份分别记录。不要为本测试隐式切模式、使能 FPU、修改 PMU 或读取带副作用的寄存器。

TUI 入口：System Regs 选择条目后按 `t`，方向键/滚轮滚动到 Latest read attempt、Retained raw value origin 和 CPU 信息；Esc 关闭。Headless 可用当前工程启动 `debugtui --project <工程.toml> --headless --stdio`，以实际 CLI 帮助确认项目参数。先发送 `{"id":1,"method":"connect","params":{}}`，再发送 `{"id":2,"method":"registers_list","params":{}}`；后续请求的 `context` 必须复制列表返回的当前 context，切核/换帧后重新获取。退出发送 `quit`。禁止把本文的示例端口当作真实板级配置。

| ID | 操作与独立核对 | 通过条件 | 状态 |
| --- | --- | --- | --- |
| PROV-H01 | 核对目录路径/SHA、配置 CPU 和显式 Probe caps 的 MIDR；在 STOPPED、运行、重连和切核后查看详情 | Catalogue CPU 与配置选择分开；Observed CPU 仅来自当前停止上下文的身份，未知/过期不猜测 | SKIPPED |
| PROV-H02 | 读一个命名 GDB 通用寄存器与有空洞索引的扩展寄存器；对照 MI 日志/target description | provenance 为 gdb_register，名称、索引、实际已选 endpoint、context、命令和时间相符 | SKIPPED |
| PROV-H03 | 在受控独立调试会话用 Console 操作连接/符号，再手工重读 | 历史来源保持原值；新请求 selected endpoint unknown，独立 configured endpoint 保留；不解析 Console 猜连接 | SKIPPED |
| PROV-H04 | 分别用显式 CP15/MRRC、命名 GDB 64 位 fallback 和已验证通用 Backend 读取 | 声明 reader 与实际路由分开；fallback 显示 GDB 名称/索引；TCL 请求显示实际请求 target/endpoint | SKIPPED |
| PROV-H05 | 合法模式读已实现 banked 视图，核对后端协议与目标恢复 | 银行协议来源不能变成 get_reg；请求 target 与恢复前后 target 独立核对，拒绝项无虚假访问 | SKIPPED |
| PROV-H06 | 合法 VFP 条件下在同批读取 D0/D1/S0/S1/Q0 | 与原始 pair 字对照，所有重叠视图保留实际首次 pair 操作/时间；不增加重复 pair 读取 | SKIPPED |
| PROV-H07 | 显式 Probe caps、EL1/EL2 MPU 总览、合法选择器索引分别执行 | acquisition 区分；每个有效/失败请求有对应来源；能力未知/权限拒绝项 access null；选择器事务恢复证据齐全 | SKIPPED |
| PROV-H08 | 默认 GDB MMIO 与一个显式 DAP/AP 通道分别读同一安全只读地址 | provenance 对应实际 GDB 或 TCL channel；地址、endpoint、请求 target、配置来源一致，不因 UI 选择改变实际通道 | SKIPPED |
| PROV-H09 | 在有独立参考字的 RAM fixture 核对 LE/BE 32/64 位与 alias；64 位期间不改变参考数据 | 两个 32 位 bus 读的 count/width 与日志一致、明确非原子；别名记录根到子偏移且复用父来源 | SKIPPED |
| PROV-H10 | 至少两核、最好两个 cluster；Scope All 读取 core/cluster/chip 并切核；比较各 worker 日志；移除某个 cluster 映射再读 | 不广播值读取，各条显示真实 Sample core/route/Owner；共享值不冒充当前核私有值，unknown owner 不发送 | SKIPPED |
| PROV-H11 | 使用独立测试服务注入连接失败、服务拒绝、发送后断连、完整错误和畸形返回；已有旧值时连续失败 | planned/started/responded 与实际字节/响应边界一致；收到错误不计 Valid；旧值来源与最新尝试分开，旧来源缺失时显示 Unknown | SKIPPED |
| PROV-H12 | Windows Terminal PowerShell 与 VS Code 终端，100×24 和约 35×12，中文目录/名称；逐页查看来源详情 | 字段/地址/target/原始高位可完整滚动；内部 TCL 协议脚本不出现在弹窗，查看详情无新增 MI/TCL 请求；这是 REG-208 的相关输入，不能单凭本 case 宣称全部视觉要求完成 | SKIPPED |

PROV-H11 还须覆盖：peer 活动使共享新值被丢弃，随后该地址读取失败，再查看中间 Snapshot、Response 和 frame/status Snapshot。只能保留该路由最后已认可的原值与来源，已丢弃的字节和来源不得复活。

对齐 JSON 时检查 `provenance.access.command` 为实际 MI/TCL 值操作（不含 framing），`access.context` 为实际 worker，`timestamp_ms` 为该请求进入发送的会话毫秒时间；planned 尚未发送时为路由解析时间。读取成功不等于 endpoint/target 证明物理身份；必须用板级独立记录交叉核对。任何恢复结果未知都按既有 FAULT 规则处理，不自动重试。
