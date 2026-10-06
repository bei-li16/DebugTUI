# BUS 读取边界环境 case

2026-10-06：三项均 **SKIPPED（按任务约束不执行上板）**。这是 BUS-H01–03 的边界补充，不代替全部跨面板验收。软件证据见 [读取边界](../../docs/bus-read-boundary.md)。

使用隔离项目，声明每个物理核的 GDB endpoint 与已独立确认的 AP target。准备固件 RAM 中已知的 32/64 位值及邻接哨兵，记录 ELF/镜像哈希；AP 运行读取只用于板级配置已经验证允许的通道。不要向生产 MMIO 发送未知副作用读取。

启动：`debugtui --project <隔离项目.toml> --headless --stdio`，按行发送 JSON 请求并保存全部 stdout/MI/Tcl 日志。先 connect，双核项目显式 select_core，再 registers_list 保存 context。请求 ID 须唯一；控制与读取不假定可在同一 worker 上并发。

| ID | 操作 | 通过条件 | 当前状态 |
| --- | --- | --- | --- |
| BUS-BH01 | core0、core1 各在同一停止点用 memory_read 读取同一固件 RAM 的 GDB/AP 路线，分别验证 32 位及 64 位大小端；再 memory_dump 读取包含邻接哨兵的 16 字节 | 值与独立基线一致，Context、实际命令、AP target/endpoint 与被选路线一致；64 位 AP 为 32×2，范围为 8×16，均声明非原子；Scope All 仍只读取选中核 | SKIPPED |
| BUS-BH02 | 保存 core0 context，切到 core1 后发送携带旧 context 的 memory_read/memory_dump；在独立安全控制通道上使被测核在 AP 响应完成前运行/停止或改变线程，记录事件顺序；稳定运行时只手动读取已批准通道 | 旧 context 在 transport 前拒绝；已接收的中途变化通知使结果拒绝，不发布有效值；无隐藏 MI 查询、halt、resume、目标重选或静默回退；稳定 RUNNING 的批准 AP 不被额外暂停 | SKIPPED |
| BUS-BH03 | 用隔离服务/故障注入网关制造短响应、非数字、超宽字及断线；清除故障后由用户显式发起下一次读取 | 首次失败有原始原因，无自动重试/通道回退；格式错误连接被丢弃；下一次显式请求可重连；旧值保持旧来源且显示失效；修正后原暂停/单步能力正常 | SKIPPED |

下面只是 JSON 格式示例。必须用 registers_list 的当前 context 替换示例 context，地址及 channel 来自隔离工程与独立固件定义，不能直接复制示例地址访问硬件。

```json
{"id":10,"method":"memory_read","params":{"address":536870912,"bits":32,"little_endian":true,"channel":"ap-core0","context":{"session":1,"generation":1,"core":"core0","frame":0}}}
{"id":11,"method":"memory_dump","params":{"address":"0x20000000","count":16,"channel":"ap-core0","context":{"session":1,"generation":1,"core":"core0","frame":0}}}
```

同一 worker 会串行执行请求，不能把其后的 continue 请求当成“读取途中变化”。BH02 的并发控制须经环境允许的独立通道或测试网关安排，避免两套调试器同时占用探针。没有此条件时单列该步骤 SKIPPED，不能用返回后的状态变化替代。

报告逐项记录步骤、配置/独立基线、真实请求与通知顺序、成功 receipt 或错误、旧样本状态、未执行原因。动态计数器按采样区间及高位一致性核验，不要求跨时间相等。所有未知 AP 权限和核归属保持未验证。
