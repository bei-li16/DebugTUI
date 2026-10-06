# BUS 绑定与来源环境 case

2026-10-06：三项均 **SKIPPED（未上板）**。属于 BUS-H01–03 的绑定/来源补充，不是完整 BUS 验收。软件范围见 [绑定生命周期与面板来源](../../docs/bus-read-provenance.md)。

使用隔离工程、与固件匹配的 ELF 及已有 GDB/Tcl 调试服务。禁用启动、连接和退出时的运行/复位/下载动作，设置 `[session] on_exit="disconnect"`；每个待测核心已经停止。准备核心独占、无读副作用的 RAM 标量，记录固件/工具/ELF 哈希、独立地址/位宽/原始值和按地址顺序的字节基线。Cache 未一致时不能把 GDB/AP 值不一致解释成读取实现通过。

默认执行不连接、只生成三个 skipped：

```powershell
node scripts/test-bus-provenance-hardware.cjs
```

复制 [case 模板](../fixtures/bus-provenance-board.example.json)，替换每核真实核心名称、变量、地址、值、字节、通道、target、GDB/Tcl endpoint 及 `memory_channels.source`，移除 `software_example`。单核 core 名称填写 `default`；多核支持 1–4 个列出的核心。fixture_ram/startup_actions_disabled 是操作者确认的准备条件。64 位 expected 使用精确十六进制字符串，expected_bytes 必须另填独立字节基线；地址限 JavaScript 安全整数范围。

```powershell
node scripts/test-bus-provenance-hardware.cjs --run --binary G:/Tests/debugtui.exe --project G:/Tests/isolated-bus.toml --case G:/Tests/r52-bus.json
```

驱动不执行 continue、pause、step、写内存、赋值、复位或烧录；仅依照显式项目连接、解析、读取，并在第三项显式断开/重连。前序失败则后续依赖 case skipped，保留原始错误、JSONL 和报告；不为使结果通过而重试读取。

| ID | 自动步骤 | 通过条件 | 状态 |
| --- | --- | --- | --- |
| BUS-PH01 | 逐核选中，设置 Scope All，添加 RAM Watch，解析绑定；读取 GDB/AP 标量及 AP byte range | 绑定属于选中 worker；数值/完整 raw/字节与独立基线一致；receipt 的 Context、epoch、实际 target/endpoint/source/命令、宽度、大小端及时间一致；声明 non-atomic | SKIPPED |
| BUS-PH02 | 修改绑定地址、大小端及句柄类型；同根再次解析后提交旧句柄 | 负向请求均拒绝，不发送 MI；新绑定不混用旧类型地址。不因 Scope All 广播其他核 | SKIPPED |
| BUS-PH03 | 显式断开/重连后逐核提交旧绑定，再重新解析并读取 AP | 旧 Context/句柄在 transport 前拒绝，新的 session 不同；新读取重新满足基线与完整 receipt | SKIPPED |

连接和清理分别记录 setup/cleanup 阶段；正常完整执行共五个阶段。软件夹具通过 `--software-fixture` 标明不是板卡测试，默认 report.passed 为 false（三项 skipped）。

配套人工步骤：在 45×12、80×24 和宽终端的 Watch/Peripherals/Memory 打开 Access；验证首次 configured、成功 sampled、错误后 retained/stale 与原 route/时间，SVD 字段沿用父寄存器实际来源。改通道 target/endpoint 或切线程/帧/核后检查旧值隐藏，取消配置不改 profile，保存后重启恢复有效覆盖顺序。Memory 自定义范围运行时只有手动读取；读副作用外设没有定时请求。

运行中绑定采样需单独准备已声明 while_running 的安全通道，由操作者明确恢复运行并保存独立状态日志；同帧静默线程/PC 或读取中通知需独立代理/线程控制条件。当前自动驱动只验证已停止 RAM；这些步骤仍未上板，不能用上表三个阶段或四核软件模型代替全部 BUS-H 的实板要求。
