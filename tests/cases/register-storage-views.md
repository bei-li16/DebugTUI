# S/D/Q 与特殊浮点环境用例

全部 **SKIPPED**，本任务不连接板卡。软件模型及离线对象编译不能替代以下结果。使用真实独立正常固件RAM参考、每核私有ready和原始工具/配置/ELF哈希。

| Case | 准备与动作 | 验收 | 状态 |
| --- | --- | --- | --- |
| VIEW-H01 | 明确D16、正常Hyp启动完成FP权限和EN；读取全部S/D/Q | S0–S31/D0–D15一致；D16–D31与Q明确No；没有高寄存器数据指令 | SKIPPED |
| VIEW-H02 | 明确D32/NEON，初始化64个逻辑字，再只读全部80项视图 | S/D/Q对应独立RAM；每个pair raw/MVFR/FPEXC和当前owner/context/route/区间可查；每pair只传输一次 | SKIPPED |
| VIEW-H03 | 按标本初始化正负零/无穷、正负quiet/signaling NaN及payload | Float32/64及四/二lane显示精确；切换原始十六进制后原值不变；格式操作零目标读写 | SKIPPED |
| VIEW-H04 | 初始化subnormal、极值及不同高低字；大小端各一套固件 | 逻辑lane0始终低位，存储和S/D/Q关系一致；科学记数值可往返原始位 | SKIPPED |
| VIEW-H05 | 两核不同FP标本、Scope All、切核及旧context请求 | 只读选中owner；peer数据/控制和配置不变；旧请求不能污染另一核 | SKIPPED |
| VIEW-H06 | 新会话中无法取得/矛盾/未适配MVFR，或者FP未使能 | 实现Unknown与禁用原因分开；自动请求无数据访问；手工失败不伪造pair或Q能力、不自动使能 | SKIPPED |
| VIEW-H07 | 已取得有效数据后故障/拒绝刷新、换帧/重连/复位 | 最近有效值保留原pair/能力/owner/时间；当前失败不冒充成功；生命周期清除过期绑定 | SKIPPED |
| VIEW-H08 | PowerShell/VS Code宽屏、小窗、中文说明与高对比度 | 分组/原始值/浮点与向量菜单、说明、状态、旧值来源可读；无目标格式命令 | SKIPPED |

实际只读驱动入口：

```powershell
node scripts/test-register-vfp-hardware.cjs
node scripts/test-register-vfp-hardware.cjs --run --binary PATH\debugtui.exe --project PATH\project.toml --core core0 --case PATH\vfp-case.json
```

从 `tests/fixtures/register-vfp-board.example.json` 复制并填写真实独立期望。当前驱动的权限范围为已适配Hyp；EL1/Guest和新鲜当前Debug EL适配仍在REG-304待完成，不按停止前CPSR推导权限。未知环境保持SKIPPED。

`tests/fixtures/register-storage-patterns.S` 仅用于专用**汇编启动代码**：先由测试startup明确建立权限/EN，再调用 `debugtui_storage_pattern_seed`，继而进入现有 `register-vfp-board.S` 的正常只读参考采样/ready循环。seed故意替换FP数据（含AAPCS callee-saved D8–D15），不能作为普通C函数调用；DebugTUI驱动不发送该初始化函数，也不使能FPU。D16使用 `DEBUGTUI_STORAGE_D32=0`，D32仅在真实NEON/D32能力明确时使用1。该标本初始化操作与被测只读动作分别记录。

JSON标本给出64个low-word-first逻辑字、80个原始视图与明确的特殊值显示期望；真实固件还需独立RAM采样，禁止把软件JSON本身计为芯片证据。FP数据应在采样钩子前保持固定，每核使用私有参考/ready。故障项使用专用可控仿真/注入环境，不在客户运行固件中尝试未知权限。
