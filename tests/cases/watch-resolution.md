# Watch 地址解析环境 case

2026-10-06：三项均 **SKIPPED（按目标约束不执行上板）**。使用隔离工程、与固件匹配的 ELF、已经验证的调试连接与 AP 通道，准备 RAM 的 uint32_t、int32_t、float、double、结构成员与数组；不得用有读副作用的 MMIO 冒充 RAM。记录配置、工具、ELF 和固件哈希。源码软件验证见 [地址解析边界](../../docs/watch-address-resolution.md)。

启动 `debugtui --project <隔离项目.toml> --headless --stdio`，向 stdin 逐行发送 JSON 并保存完整输出。下面先添加 `counter` 到 Watch，再取得当前 Context；在支持的环境里用 `select_core` 选择核心后，重新请求 registers_list，替换示例的全部 Context 字段。

```json
{"id":1,"method":"connect","params":{}}
{"id":2,"method":"watch","params":{"expression":"counter"}}
{"id":3,"method":"registers_list","params":{}}
{"id":4,"method":"watch_resolve","params":{"expression":"counter","path":[],"context":{"session":1,"generation":1,"core":"core0","frame":0}}}
```

| ID | 操作 | 通过条件 | 状态 |
| --- | --- | --- | --- |
| WATCH-H01 | 每核分别停止，解析全位整数、浮点、结构成员、数组孩子和 `*(uint32_t *)(已知RAM地址)`；用成功 address/bits/little_endian 发起明确 channel 的 memory_read。四核项目分别选择 core0…core3，设置 Scope All 再复测 | 解析 Context/thread/frame_address 属于选中连接，地址与独立 ELF/DWARF/固件基线一致；原 GDB 调用策略在每次成功和普通错误后相同；AP 值与独立 RAM 基线一致；All 不广播地址解析或读取 | SKIPPED |
| WATCH-H02 | 用旧核心或旧停止 Context 请求 watch_resolve；请求孩子索引 4096、九层路径；经可控代理，在地址响应返回前插入运行/再次停止/同帧线程通知，或在清理前使线程/帧/PC 改变 | 旧 Context/越界路径在 MI 前拒绝；中途变化不返回可用绑定；不会隐式 halt/resume/切帧/切核；已知变化后不再发送地址或类型探测；最终执行对象清理和原调用策略恢复 | SKIPPED |
| WATCH-H03 | 隔离 MI 代理注入 path_expr 为调用或自增、var-delete 错误/ndeleted=0、调用策略恢复错误；发送 channel 为 null/数字的内存请求。另用 Rust 控制端保留 Request 克隆，在代理确认地址 dispatch 后调用 cancel_read，再释放响应 | 路径二次校验在数据求值前拒绝；清理/恢复失败进入 FAULT 且无有效绑定，重连是显式操作；错误 channel 不发送 GDB/AP 读取；取消仍完成清理/恢复，后续新请求可正常执行；不取消写入或会话 | SKIPPED |

不要直接向已有 Watch 添加会修改目标的表达式来制造 H03：普通 GDB Watch 的原有显式求值行为不属于本批地址解析保护区。软件夹具通过已配置根名称测试解析前置拒绝；板级负向测试应在隔离代理中替换返回路径，保证目标程序不会先执行被测函数或自增。

同一 worker 串行处理请求，不能把后发的 continue 当作解析中的变化。没有独立可控代理或安全线程控制条件时，H02/H03 相应步骤继续 SKIPPED；不把返回后的线程变化替代中途证明。后续 [绑定与来源 case](bus-provenance.md) 已准备缓存生命周期子集的可执行主机驱动，默认 skipped；完整 BUS-H 场景仍待补齐。
