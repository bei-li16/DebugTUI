# R52 常用定义与容量的延后环境 case

本 Goal 不执行上板；六项 case 当前全部 **SKIPPED**。执行前必须完成当前 Debug 权限后端适配，使用专用固件及客户配置副本。记录真实 R52 revision、独立 MIDR/MPIDR、每核 AP/target/endpoint、工具版本及 SHA256、CPU/目录/有效来源、project SHA256，以及来自另一调试器或固件的独立原始值基线。两个核不得复用同一个私有 target；不能把本次 DebugTUI 输出复制为期望值。

正常执行的访问条件与 Debug 状态分开检查。无法证明实际权限时，预期是在数据指令前 Unknown/Restricted，不改模式/控制获取访问，也不把此结果当成功读通。当前软件模型证据不升级硬件 verified。

| ID | 步骤与独立预期 | 状态 |
| --- | --- | --- |
| R52-MODEL-H01-IDENTITY | 分别选择两核，在停止 physical frame 0 Probe；读取 MIDR/MPIDR，核对独立原始字节及每个字段。Aff0 不等于配置 core 序号的假设不得成立；保留实际 variant/revision，source conflict 可见 | SKIPPED |
| R52-MODEL-H02-CONTROL | 显式读取 SCTLR/HSCTLR/CPACR/HCR，核对独立 baseline 与字段；TRVM bit30 与 TVM bit26、SCTLR.FI RO 副本、cp11 ignored/UNKNOWN 解释正确。查看、展开及格式操作不写任何配置 | SKIPPED |
| R52-MODEL-H03-CAPACITY | 每核 MPUIR/HMPUIR 独立 Probe，验证 EL1 DREGION 与 EL2 REGION。按实际 build 的 16/20/24 或零 EL2 容量验收；未提供对应 build 的容量组合单独 SKIPPED，不用单板结果覆盖所有配置 | SKIPPED |
| R52-MODEL-H04-REGIONS | 先保存两核 PRSELR/HPRSELR 的独立初值，再显式 direct MPU 读取，只请求小于各 bank 容量的 region；核对全部 BASE/LIMIT 与权限，index15/16/23 等仅在实现时检查。EL2 零容量不得请求 region 或写 selector；另一核字节不串用 | SKIPPED |
| R52-MODEL-H05-MAIR | 读取 MAIR0/1/HMAIR0/1，对照固件各八个 attribute bytes 与 region AttrIndx；Device 的四种编码、Normal 内外 cache、保留/UNPREDICTABLE 编码正确保留。MAIR 的低/高半不混用，不改变 attribute | SKIPPED |
| R52-MODEL-H06-PRESERVATION | 回读 selector/控制并核对初值与 project SHA256；direct 读取不改变 selector，查看/切核不广播。失败值不是 0，旧值保留原核/时间/来源并 stale；记录数据请求的完整 target/endpoint/context | SKIPPED |

可使用 headless JSONL 的现有生产接口：`select_core` → `registers_list` 获取当前 context → `registers_probe` → `registers_read`（传相同 context 与本表 IDs，必要时 manual=true）→ `registers_mpu`（bank=el1/el2，read=true）→ 再次 `registers_read` 回读控制与 selector。先查询接口实际支持和当前上下文，不复用另一核/旧会话 context。既有 `scripts/test-register-capabilities-hardware.cjs`、`scripts/test-register-selectors-hardware.cjs` 的独立 case 可用于身份/容量和 selector 的补充验收；selector transaction 的临时写与恢复需另按 C05 验证，不能当 direct 读取的行为。

每个 case 保存请求/响应 JSONL、MI/Tcl 日志、独立 baseline、EXE/project/工具摘要和 pass/fail/skipped 报告。结束时关闭专用会话，保留原项目并回读已临时使用的 selector；恢复不确定则保留 FAULT/隔离诊断，停止注入并按专用板级恢复流程重连，不盲目重试。目录源码核对不能代替本表的实板验证。
