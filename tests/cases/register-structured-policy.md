# 结构化条件与 PPB 环境用例

以下用例均 **SKIPPED**，未上板。软件证据为 `tests/register_policy.rs` 和 `registers::policy::tests`。这些步骤用于环境允许后的独立检查，不以软件原始响应替代实板结果。

准备项目副本、已复核的核描述、独立源寄存器预期值、target/AP 映射、两个核的连接端口，记录 DebugTUI 提交/版本、OpenOCD/GDB/探针版本、板卡与 revision。保存原配置 SHA256。目标由执行人员预先准备为 STOPPED/frame 0；本用例不通过改模式、权限或使能位创造可读条件。先以人工已核实的目录执行，待 B01/B09 完成后也验证对应内置 M 目录。

运行 `debugtui --project <项目副本> --headless --stdio`。请求是逐行 JSON，例如：

```json
{"id":1,"method":"connect","params":{}}
{"id":2,"method":"select_core","params":{"name":"core.2"}}
{"id":3,"method":"registers_list","params":{}}
{"id":4,"method":"registers_read","params":{"ids":["mpu_type","demcr"],"manual":true}}
{"id":5,"method":"registers_read","params":{"ids":["mpu_ctrl","dwt_ctrl"],"manual":true}}
{"id":6,"method":"registers_matrix","params":{}}
{"id":7,"method":"quit","params":{}}
```

ID 按核描述中的实际 ID 替换，case 4 的源寄存器必须可读且无读副作用。每个步骤等响应后再发下一条，保存 JSONL 与 MI/Tcl 日志。对照独立预期值、宽度、source、owner、context、provenance.access 的实际 endpoint/target/channel/address；不能只检查“有返回值”。

| ID | 操作与独立条件 | 预期 | 状态 |
| --- | --- | --- | --- |
| POLICY-H01 | M7＋M4 的各核 owner 绑定不同 AP/target；逐核手动读相同 CPUID 地址，对照 TRM/实板独立身份记录；切回原核 | 核型号及 raw 匹配各自实际核心，路线和缓存不串核；不能仅因两个 raw 相同证明同型核映射正确 | SKIPPED |
| POLICY-H02 | 两个独立前置场景：MPU_TYPE 的 DREGION 为 0、及已核实非零；先读源，再读 present_if 项与 Alias；另在尚无源观测时请求一次 | 0 为 HardwareNotImplemented；非零只读合法范围；无证据为 Unknown；后两种拒绝的 access 为空，无数据读请求；Alias 不绕过父项 | SKIPPED |
| POLICY-H03 | 固件预先分别准备 DEMCR.TRCENA 为 0/1 的停止场景，不由 DebugTUI 置位；先读 DEMCR，再读 DWT_CTRL 和低位 Alias | 0 为 FeatureDisabled/NeedEnable 且零 DWT I/O；1 在实际模块可用时成功；本用例不增加使能写入；Alias 原值与父位段一致 | SKIPPED |
| POLICY-H04 | 定义中 DHCSR/SysTick CTRL 已标读副作用；先 `manual=false`，核对调用日志；需要单次实读时由执行人员明确决定，并记录读清影响 | 自动请求为 NotRead、无数据 I/O；手动最多一次，记录实际返回和对 OpenOCD 状态的影响；不加入后续自动队列或复位值扫描 | SKIPPED |
| POLICY-H05 | 在副本目录创建 scope=unknown、无 writer 的普通 GDB/MMIO 描述；手动及自动请求；再分别配置共享 ppb 通道、错 owner、非零 binding base | 未知归属不生成 owner、不访问；错误 PPB 路由返回明确 ReaderUnsupported，不回退全局通道；原配置保留 | SKIPPED |
| POLICY-H06 | 配置声明 cpu.mode=Hyp/cpu.debug_el=2，给通用项声明 min_el=2；当前通用入口缺生产证明；并在纯只读 Status 查看规则/来源 | 请求在数据指令前 Unknown；声明值/保存 CPSR 不授予访问；Status 不补读依赖。生产 R52 证明成功另按 C03 专项用例执行 | SKIPPED |
| POLICY-H07 | 修改只读源的前置状态后重新停止/重连，或制造源读取失败；记录旧值；切核/换帧后查看条件，再显式重新读源 | 旧源不复活；过期条件 Unknown；失败旧值和原来源保留；新的合法源决定后续条件，迟到回复不改当前值 | SKIPPED |

结束仅 `quit`/disconnect，核对日志没有本用例引入的 halt/resume/模式/enable/持久写入。比较原配置 SHA256，保留报告及失败原文。若发生读取异常或状态未知，停止该场景，不改后端盲目重试；恢复/复位由执行人员按板级流程另行决定。
