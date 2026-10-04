# REG 状态与取消：延后环境验收

全部初始状态为 **SKIPPED / 未执行**。软件 Ratatui、MI 与 Tcl 证据不替代以下实际环境结果。这是可人工执行的 case；Headless JSON 没有取消 API，不宣称有自动上板取消驱动。

准备：记录 DebugTUI／GDB／OpenOCD 版本和哈希、项目与目录来源、固件／ELF 提交、MIDR、探针、owner 和暂停上下文。用专用测试固件在允许的物理模式及 frame 0 暂停两个核心。可选类别须有匹配协议及合法访问前提；不得为测试使能 FPU、修改 MPU／PMU 配置或切换 CPU 模式。用独立固件或已审查原生后端记录 target／选择器／PC／CPSR／scratch／peer 基线，不以 DebugTUI 响应作独立预期值。

| Case | 操作 | 验收条件 |
|---|---|---|
| REG-T07-STATUS | 展开 Core 和 CPSR 字段，搜索、分类、Target/All、折叠、滚动，打开 Status | Total 固定；Shown 随展开／筛选变化，字段／标题不计；统计不新增读取，分类合计等于 Shown |
| REG-T11-STATUS | 查看实际缺失、权限受限、无 reader、失败和 WO 项；手工读允许项 | 状态与原始原因相符；缺失和 WO 无读取；Target 隐藏已知缺失，All 留定义；旧值带失败状态且不计 Valid |
| REG-T05-STATUS | 已读后继续、再暂停、切核／帧、重连 | 过期值 Stale 且不计 Valid；owner 正确；迟到响应不污染新上下文；控制动作只在专用固件允许时执行 |
| REG-T07-CANCEL | 触发读取，在 pending 期间点击 Cancel read 或 Esc，再手工重读 | 在途限制保持至回复；当前事务结束、后续项停止，取消批次无新样本／自动重试；旧值保留，手工重读可用，会话不退出 |
| REG-T06-CANCEL | Probe 后执行允许的 MPU/PMU bank read 或 MPU 总览，在 pending 时取消／关闭／换 bank | selector 和 target 恢复且独立回读一致；配置、PC、CPSR、scratch 和暂停状态不变；取消数据不显示为新的有效值 |
| REG-T06-MULTICORE | Scope All 选 core1，读期间取消；切 core0 手工读 | 只访问 core1 owner，两核保持暂停，peer 基线不变；core0 不受旧取消标记影响 |
| REG-T08-WIDE | PowerShell、VS Code 各以约 120×35 键鼠操作 | 列、焦点、状态、对比度、按钮可辨；中文来源可读；快捷键冲突有按钮路径 |
| REG-T08-NARROW | 两种终端各缩至约 35×12，滚动 Status 到底，查看中文，关闭 | 摘要可理解，各分类和来源可到达；焦点／命中区正确；缩小不新增读取 |

每例保存 MI／TCL 日志、独立前后基线、屏幕、实际读数量及状态。按取消时已完成则记录 **SKIPPED（未覆盖在途取消）**，不能算通过；可改用合法多项读取再测，不故意拖延恢复或改生产超时。硬件缺失／权限不允许按真实原因跳过。断线／恢复失败注入只在独立可恢复的实验环境执行，要求 FAULT、阻断共享访问和无自动重放；该项当前仅有软件夹具证据。
