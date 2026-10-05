# 银行当前 Debug 状态环境用例

全部 **SKIPPED**；本任务不连接板卡。软件模型、离线编译及命令检查不能替代这些结果。默认驱动四阶段SKIPPED；仅显式 `--run` 会连接工程。使用专用只读固件的ready循环，记录固件/ELF/工具/配置哈希，启动前确认目标已停止。工程Tcl target映射必须由板级配置明确给出。

| Case | 环境与动作 | 要求 | 状态 |
| --- | --- | --- | --- |
| BANK-P01 | R52 Hyp hook，依次读二十三个内置银行 | 每项和独立固件RAM参考相等；外部MIDR/EDSCR、方法、当前owner/route/context与请求时间完整 | SKIPPED |
| BANK-P02 | User hook，自定义目录加入七个`*_usr` | 七项MOV32与独立参考相等；其余受限；不注入当前CPSR、MIDR或特权银行指令 | SKIPPED |
| BANK-P03 | FIQ/IRQ/SVC/ABT/UND/System各正常固件hook | Debug EL1时Hyp银行受限，其他Unknown；外部AP身份后无CPU银行指令，不尝试模式切换 | SKIPPED |
| BANK-P04 | 测试环境在停止后以独立工具改变Debug EL，保留停止前DSPSR | 选择规则按当前EDSCR，不按旧DSPSR；例：保存User而当前Hyp允许Hyp银行；保存Hyp而当前EL1不发布数据 | SKIPPED |
| BANK-P05 | core0/core1私有参考和双核Scope All | 只读选中核心；逐核原始证明/旧值隔离，另一核R0/PC/控制不变 | SKIPPED |
| BANK-P06 | 在专用仿真/故障注入环境改变DSPSR/DLR/身份/EL或破坏R0恢复 | 首个不确定结果停止、target隔离；无部分值、重试、后续CPU指令或自动回滚 | SKIPPED |
| BANK-P07 | 非物理frame、取消及读取途中实际thread/frame变化 | 发起前拒绝或完成已发出的恢复后丢弃结果；旧值保留原owner/context/证明 | SKIPPED |
| BANK-P08 | 旧后端v1、错误协议、未适配CPU及正常失败刷新 | 数据指令前拒绝旧协议，不回退；失败旧值与原证明保留，安全拒绝不拖垮Core/调试控制 | SKIPPED |

执行入口：

```powershell
node scripts/test-register-banked-hardware.cjs
node scripts/test-register-banked-hardware.cjs --run --binary PATH\debugtui.exe --project PATH\project.toml --core core0 --case PATH\banked-case.json
```

复制 `tests/fixtures/register-banked-board.example.json` 填写真实 `expected_midr`、停止前 `expected_mode`、**独立声明的当前 `expected_debug_el`**、frame hook、peer和参考符号；不得从停止CPSR自动推断Debug EL。Hyp保持二十三个参考；User全部非User银行设置`unavailable:true`。EL1中Hyp项`unavailable:true`，其他项`unknown:true`。受限/未知项不求值RAM参考。

User完整三十项case在原banks数组末尾加入r8_usr、r9_usr、r10_usr、r11_usr、r12_usr、sp_usr、lr_usr，独立参考索引依次23–29，七项不标unavailable/unknown。原始固件 `tests/fixtures/register-banked-board.S` 的参考空间为120字节，每核需不同私有参考/ready符号；编译时设置 `DEBUGTUI_BANKED_MODE`，startup建立模式及银行数据。钩子只在正常固件运行，ready循环中暂停，禁止在函数入口采样。

自定义目录从R52目录复制，新增`usr`组（parent=core）及上述七项：各项id/name为对应名称、group=usr、bits=32、scope=core、access=ro、reader={kind=banked,name=对应名称}。保留Core控制及原二十三项以便驱动完整检查；测试工程明确设置catalogue路径和banked_command。无需改客户原工程，可使用独立测试工程。

BANK-P04需要外部测试环境进行受控准备；DebugTUI驱动不发送DCPS/MSR/CPS，不建立这类状态。标准驱动负责P01/P02/P03/P05和正常旧值检查，故障/异步环境项按表执行并保留日志。真实硬件未授权或不可用时保持SKIPPED。
