# 总线读取的上下文与响应边界

2026-10-06，开发分支 `codex/register-debugging`，版本仍为 0.9.3。本批是 BUS-005/006/007 的软件修复子集，BUS-001–008 尚未整体验收。

## 修复的问题

AP 的 Tcl 请求阻塞期间，GDB 的异步通知进入接收队列，原标量读取没有在返回值前处理通知。范围读取虽然比较上下文，但未消费队列，可能仍看到旧状态。同一停止点、同一帧编号的线程切换也不会改变原 Context。另一个缺口是旧 GDB 外设路径只解码 contents，没有核对响应地址。

`memory_read`、`memory_dump` 和原 GDB `peripheral_read` 现在共用读取边界。开始时处理已排队的通知，核对可选 Context 和取消标记；完成时再次处理通知，比较会话、核心配置、停止代次、帧、状态和通知代次，再检查取消。读取途中运行、停止、同帧线程切换、线程退出或断线均使结果失效。稳定 RUNNING 状态下、明确声明 while_running 的 AP 仍可读取；边界不发送额外 MI 查询、halt、resume 或 target-selection。

GDB 标量响应按范围规则验证合法字节、完整长度、连续块及精确起始地址，再按声明的大小端解码。AP 标量和范围响应出现非数字、宽度或数量错误时丢弃该连接，不自动重试或回退。下一次显式读取可重新连接。

首次原生 Memory 回归发现旧标量 tracked 入口及本批范围入口把纯 read_memory 的断线当成选择器/写入未知状态，隔离整个服务，使下一次显式读取也被拒绝。现为系统生成的 target-specific read_memory 增加保留进度的只读事务入口；断线只丢弃连接。服务已有故障或明确 target/core restoration failure 仍保持隔离，写入和选择器事务继续原来的严格规则，不因读取重连清除其他事务的故障。

## 请求与证据

`memory_read` 和 `memory_dump` 接受 `registers_list.context`；旧客户端省略 context 的兼容行为保留。Watch/Peripherals 的普通读取请求现在携带发起时的 Context。原 `peripheral_read` 继续表示 GDB 通道，选择 AP 使用 `memory_read`，不得传非空 channel 再悄悄走 GDB。

成功响应保留原字段，并增加 `context` 与 `access`。Access 记录实际 MI/Tcl 命令、route、请求上下文、started/responded 阶段和会话相对时间。标量 AP 64 位读取仍为两次 32 位总线操作，atomic=false。范围使用 8 位操作，bits 为整个区间的位数，count 为字节数；byte_order=little 在范围 receipt 中只是地址序字节约定，不对原始字节重排。

GDB 的 `access.route.endpoint` 只有在 DebugTUI 已选择连接、其后没有不透明 Console 命令时才有值；否则为 null。`configured_endpoint` 和兼容顶层 endpoint 是配置提示，不能据此证明实际连接。AP route 的 target 是实际放入命令的显式目标名，不能证明板级 AP 与 CPU 的硬件关联。Context.core 同样是配置归属，不是本批新增的物理身份观测。

## 软件验证

六项新增单元分别验证：实际 TCP dispatch 后注入 MI 队列通知，十种标量/范围组合均拒绝旧响应；全局/单请求取消在 dispatch 后拒绝；旧 Context 和发送前取消不接触 transport；非数字/超宽响应删除连接且无重试或 GDB 回退；断线后的第二次显式请求才重连；只读 transport 错误可恢复而 target/core restoration failure 继续隔离。通知注入在服务端收到请求后、回复之前完成，不依赖睡眠来制造竞态。原有精确 64 位路线和范围高地址测试增加 Access/时间/上下文断言。

一项新增真实 MI 管道集成以八个独立进程场景验证 memory_read/peripheral_read 的大地址、双块响应、大小端、旧 Context、错误地址、短响应和读取中 RUNNING 通知。原四核 worker/Scope All、MMIO 来源及 STM/GIC/Probe 测试随完整 Cargo 回归检查；原生 GDB Memory 与 SVD 外层套件单独执行。

完整结果与限制写入 [开发进度](registers-development-status.md)。[三项环境 case](../tests/cases/bus-read-boundary.md) 均未执行；软件夹具不代表芯片的 AP 运行能力或实板身份已验证。

## 未完成范围

只处理响应边界前已经到达的通知；返回后发生的状态变化仍须由视图/协调器的后续快照失效，不将主机采样区间声称为硬件原子快照。没有为每次 AP 读取额外查询 GDB 的物理线程或运行状态。

后续 [Watch 地址解析](watch-address-resolution.md) 已补齐解析期间的函数调用策略、线程/帧/PC 证明及严格清理，并将局部取消接入实际 Session/Coordinator 请求；[绑定与读取来源](bus-read-provenance.md) 补齐解析后句柄、同帧线程代次、路线指纹、统一覆盖及实际 receipt/保留来源，并准备三项子集环境驱动。新增系统 MMIO 全模块、混合服务竞争、完整 BUS-T/H 和 Issue #1 验收仍未完成。因此不勾选 BUS 整项，不增加已完成 feature 数，不发布 Release。
