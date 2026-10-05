# 多核共享寄存器归属人工 case

状态：**八项全部 SKIPPED，未执行上板测试**。软件证据及配置规则见 [REG-109 自检](../../docs/register-shared-owners.md)。环境具备后使用开发版 DebugTUI、正确的多核工程、实际 GDB endpoint／OpenOCD target 和已独立确认的 core→cluster 拓扑。至少两个同 cluster 核心及另一个 cluster；没有第二 cluster 的子项单独记录 SKIPPED。

准备三个确认可安全读取的板级对象：每核私有、每 cluster 共享、整芯片共享。可使用测试固件提供的 RAM 字或真实 RO 寄存器；目录明确 reader／地址／scope，alias 与父 scope 一致。分别从独立固件基线或调试工具取得原字节和各对象的物理归属，不能只用 DebugTUI 返回值作为预期。停止相关 CPU 不代表停止 DMA、计时器或其他总线主设备；采样不声明硬件原子性。

保存开发提交／可执行文件哈希、工程及目录、实际拓扑、每核 endpoint／target／通道、独立原字节、MI/TCL 日志、CLI JSON 和终端截图。JSON 验收可依次发送：

```json
{"id":1,"method":"select_core","params":{"name":"core0"}}
{"id":2,"method":"registers_list","params":{}}
{"id":3,"method":"registers_read","params":{"ids":["private","cluster","cluster_alias","chip"],"scope":"all"}}
{"id":4,"method":"status","params":{}}
```

ID 及核心名称替换为实际目录定义。检查 `owner`、`context.core/session/generation`、`view`、`source`、`owner_generation` 和 `owner_generations`／`register_owner_generations`，以及原始值、状态和时间。Scope All 只读当前采样核，不广播。

| Case | 操作及明确预期 | 状态 |
|---|---|---|
| OWNER-H01 三种归属与两 cluster | 依次切 core0／core1（同 A）及 core2（B），读取私有／cluster／chip 对象。私有值及 owner 分核，cluster:A 与 cluster:B 不串值，chip 对象归 chip owner；Sample core／实际路由仍对应采样核。与独立原字节比对 | SKIPPED |
| OWNER-H02 原路由缓存与别名 | 两个同 cluster 核心分别读取相同 ID，再切回原核。各路由的原值、时间和有效上下文分别保留，不因另一路由覆盖而丢失；alias 等于父相应位段，一批父＋alias 仅一次物理读取 | SKIPPED |
| OWNER-H03 缺失归属 | 用独立工程副本删除一个核心的 cluster 映射，另一个副本同时移除 topology.chip 与 debug.chip。相应项目为未知归属／Unavailable，值为空，MI/TCL 没有该项目读请求；不能显示零或按 core 编号推断 cluster | SKIPPED |
| OWNER-H04 相关域失效 | 各核先采样；只让 A 中的 core1 Run／Pause。A 和 chip owner 代次提升、旧共享样本过期；B 样本及 core0／core2 私有样本继续按自己的上下文判定。只刷新 A 的一个条目不隐式读取 peer／B | SKIPPED |
| OWNER-H05 未映射 producer | 未映射核心 Run／Pause，保持其他核暂停。全部已声明 cluster 和 chip 域保守过期，其他核心私有值不因该共享失效而过期；不把未知核心猜测归入 A 或 B | SKIPPED |
| OWNER-H06 迟到结果 | 用可控管道延迟 core0 的私有＋A 共享读取，再从独立入口令 A 的 peer 活动。通知在该批回复前被观察；共享新数据丢弃，原值及原时间保留，记录的 owner 代次与当前不同；私有项按本核上下文独立判断。再用新的代次读取可成功 | SKIPPED |
| OWNER-H07 新停止点／重连／失败 | 独立产生 peer 的新停止通知，状态仍为 STOPPED；当前核无需新读即可看到 owner 代次和共享失效。仅在软件故障代理中阻塞 worker 到协调器超时，相关共享域立即失效，其他 cluster 与私有值保留。Reconnect 后各 worker session 更新、样本清空，旧请求拒绝。新 session 首次读失败时不可从旧会话导入原值；本会话读失败可保留最近已知值并明确 Error | SKIPPED |
| OWNER-H08 界面与兼容性 | 宽／窄终端中 `t` 查看 Scope、Owner、Sample core、采样视图及 Owner lifetime。换核不把共享对象显示为私有对象；未知归属明确显示 unknown。相关域变化解除其旧退避，其他私有失败不反复读。JSON 私有样本省略 owner_generation，既有 Snapshot.registers 的字段结构保留；共享 GDB 条目的首个 owner 失效通知中 error=true，原值保留且无额外读取 | SKIPPED |

自动软件入口为 `cargo test --locked --test register_shared`、`cargo test --locked --lib shared_owner` 和 `cargo test --locked --lib shared_ui`。这些命令不运行本表。人工执行后逐项填写 PASS／FAIL／SKIPPED、原因、实际环境及证据路径；目前没有实板结果。
