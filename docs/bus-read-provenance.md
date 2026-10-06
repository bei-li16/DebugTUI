# 总线绑定生命周期与面板读取来源

2026-10-06，开发分支 `codex/register-debugging`，版本 0.9.3。本批补齐 BUS-004/005/006/007 的绑定和面板来源子集；不表示 BUS-001–008 已全部验收。

## 绑定不是表达式的数值

`watch_resolve` 的只读类型解析规则见 [地址解析](watch-address-resolution.md)。成功响应新增 `binding_id` 和 `selection_epoch`；后续 Watch 标量 `memory_read` 携带 `watch_binding=binding_id`、同一 Context/selection_epoch 及原 address/bits/little_endian。worker 验证缓存中的类型地址、会话、停止代次、核心、帧、选中线程代次、ELF 路径、已知 GDB endpoint 和根 Watch 是否仍存在；不能改变位宽、地址或字节序继续使用旧绑定。

同根同孩子重新解析替换旧绑定，缓存最多 256 项。显式换帧在 MI 发送前使绑定失效，即使后续换帧失败；线程选择/退出通知、重连及不透明 Console/寄存器边界也失效。停止时，绑定读取前再次查询实际选中线程、frame 和 PC；检测到静默变化后发布新的 selection_epoch，拒绝内存 dispatch。下一次 UI 快照清除同一旧 owner 的样本和解析绑定。

稳定 RUNNING 的已批准 AP 可以使用停止时的原地址绑定，不额外查询 GDB 或隐式暂停；下一次真实停止代次变化要求重新解析。UI 的协调器 revision 与 worker Context 分开，单纯 Continue revision 改变不丢掉仍属当前 worker 停止代次的绑定。AP 读取中通知的前后边界仍适用，见 [响应边界](bus-read-boundary.md)。

`binding_id` 是生命周期句柄，不是访问权限令牌。旧 Headless 客户端可以省略 selection_epoch/watch_binding/context，直接按明确数值地址读取；已有宽度、状态、通道及通知检查仍执行。不能因为数值 API 兼容而宣称它具有 Watch 类型证明。显式传入错误类型或过期句柄会拒绝。

## 策略与来源

Watch、SVD 外设和 Memory 使用同一配置查找顺序：

1. `chip:<chip>|<core>|<item>`，其次同芯片/核心的 Watch 根策略。
2. 旧 `<core>|<item>`，其次旧核心的 Watch 根策略。
3. 无作用域的条目策略，其次无作用域 Watch 根策略。
4. 未配置时采用手工 GDB，interval_ms=0。

每个层级的孩子覆盖根。单核 UI 的旧存储键仍为 `single`，实际 worker Context 为 `default`；通道 cores 可以在单核工程声明 `["default"]`，不能写 `["single"]` 或给多核项目使用未知核心。Memory 兼容旧范围/通道键，保存仍写芯片/核心专属键。typed Watch writer 的通道检查及外设编辑器使用相同有效策略，不能通过旧键或根继承悄悄换到 GDB 写入。

外设手动、停止点自动刷新和定时读取全部走 `memory_read`，默认 GDB 也经过同一 receipt 校验。旧 `peripheral_read` Headless API 保留。WO、未知字节序、非法宽度/对齐拒绝；SVD 读副作用仅显式手动读取，不轮询。

界面用成功 `access` 校验当前 Context/selection_epoch、responded 阶段、完整时间区间、状态、实际命令和 route。标量还核对 address/bits/value 与精确 RawValue；AP 核对配置来源、target/endpoint、核心限制、bus_width/count、字节序和 non-atomic。Memory 范围额外核对实际起始地址及完整字节数组。缺失 receipt 或任一不匹配不发布新值。

尚未读取时显示 configured 路线；成功后显示实际 receipt。GDB 的 endpoint=null 显示 `unknown (configured …)`，不把配置当成已观察连接。AP target 是命令使用的目标名，source 是配置文件来源；两者均不能证明芯片硬件的 AP/CPU 关联。实际 route 的物理解释仍依赖板级验证。

同一 owner 的普通读取失败保留原值、原时间和原来源，并标明 `retained / stale` 与错误；换停止点后的失败也不替旧值制造新来源。FAULT 标记旧值失效；线程/帧/session/core endpoint、芯片、通道定义或源配置变化会清除或隐藏旧数据，迟到结果丢弃。真实 Memory 面板不回退使用无上下文的 Snapshot.memory；演示数据仍可展示。

## 软件证据与环境边界

七项新增单元覆盖四核策略优先级、default 核校验、协调器 Continue revision、同帧线程代次、芯片级外设手动/编辑路线、读取失败和新停止点旧来源、未知 GDB endpoint、错误 receipt，以及精确 64 位 AP 32×2 原始值。两项新增 MI 管道集成验证静默线程/帧/PC、参数修改、重新解析、换帧和重连；原四核 Scope All 集成增加实际绑定读取和跨 worker 句柄拒绝。

原生 Memory 套件用真实 GCC/GDB 取得绑定，20 次运行中 AP 采样均携带该绑定，验证零额外 MI；暂停后旧绑定在 transport 前拒绝。四核可重跑的软件环境驱动：

```powershell
node scripts/test-bus-provenance-fixture.cjs target/debug/debugtui.exe
```

它启动严格 Node MI 模型和本机 TCP Tcl 服务，执行同一个延后环境驱动的连接、逐核 GDB/AP/范围、参数和旧句柄拒绝、重连及清理五阶段。默认延后驱动不启动调试器，生成三项 skipped：

```powershell
node scripts/test-bus-provenance-hardware.cjs
```

[三项环境 case](../tests/cases/bus-provenance.md) 与 [配置模板](../tests/fixtures/bus-provenance-board.example.json) 已准备，实板未执行。测试计数、源码哈希及交付结果见 [开发进度](registers-development-status.md)。

完整 BUS 尚需新增系统 MMIO 全模块接入、混合 Watch/Memory/Live Watch/选择器服务竞争、完整 BUS-T/H 矩阵和 Issue #1 验收。这里的三个环境 case 是绑定/来源子集，不代替 BUS-H01–03 的全部跨面板、权限、故障和运行状态验收。不增加 TODO 整项勾选，不发布 Release。
