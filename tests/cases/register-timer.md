# R52 Timer 环境 case

初始状态：以下十四项均 **SKIPPED**。本批只执行软件验证，不上板。使用审核过的工程与固件，为每核保存提交、EXE/目录/profile/ELF 摘要、实际 GDB/OpenOCD 版本与后端协议、物理核、模式、停止位置、完整 MI/TCL/JSON 日志和截图。软件期望值不得作为芯片结果。

固件预先建立需要的合法权限、计时配置和独立基线；调试器不能为测试改变 CPU 模式、Timer enable/mask/compare/offset、计数器或选择器。自然停止于物理 frame 0，先核对模式，再显式 Probe；非法或未核验权限的项保持未读。每次运行、切核、换帧和重连后重新取得 context。读操作及取独立固件变量时不调用目标函数。

| ID | 操作与独立核对 | 通过条件 | 状态 |
| --- | --- | --- | --- |
| TIMER-H01 | 专用 Hyp 固件逐项读取十五项 Timer，按 [目录表](../../docs/register-timer.md) 独立核对编码；为四个 CVAL/offset 保存固件基线，为动态项记录采样时序 | 三十二/六十四位和编码正确，稳定项与独立基线一致；动态项按有效区间核对；没有隐式设置控制或以目录值作为基线 | SKIPPED |
| TIMER-H02 | 无声明/当前 Probe 时展开 Timer，再使用隔离工程声明 timer.present=0；显式合法 Probe 后比较声明与观察 | Unknown 自动零请求，No 手动也零请求；有效观察及原声明来源分别保存；声明排除不冒充物理缺失 | SKIPPED |
| TIMER-H03 | 已审核固件自然运行于 EL1/SVC/Guest；独立记录 CNTHCTL 与上层 trap，分别核对允许的 CNTP 和 CNTV 项 | 合法项可读；Hyp-only 项不尝试非法访问；访问拒绝不标为未实现，虚拟视图不冒充物理视图 | SKIPPED |
| TIMER-H04 | EL0 固件独立记录 CNTKCTL 的 PL0PCTEN/PL0VCTEN/PL0PTEN/PL0VTEN，覆盖允许与拒绝场景及 CNTFRQ 特例 | 四种门控分别说明；CNTFRQ 位 0/1 规则正确；没有由拒绝或频率零推断 Timer 缺失 | SKIPPED |
| TIMER-H05 | 固件预先关闭三个 Timer，保存原始 TVAL/CTL；界面展开字段并按 t 查看说明 | TVAL 与 ISTATUS 的 UNKNOWN 语义可查，完整原始位仍保留；不将任意读值当成有效时间差或已断言的中断条件 | SKIPPED |
| TIMER-H06 | 固件预先建立 ENABLE=1、IMASK=0/1 的受控场景，比较独立时间条件及 CTL | ENABLE、IMASK 与 ISTATUS 分开；ISTATUS 只读，IMASK 不改变条件状态；读不启用或解除屏蔽 | SKIPPED |
| TIMER-H07 | 受控固件建立 TVAL=0、正/负差值及符号边界；用相应位宽独立核对，切换有符号显示 | 原始三十二位不变，差值解释正确；不同时间的动态样本允许符合记录区间，不用跨时刻值制造相等断言 | SKIPPED |
| TIMER-H08 | 四个稳定六十四位 CVAL/offset 使用不同非零高字；独立 GNU Arm MRRC 固件基线与实际后端读取比较 | 全部高低位精确一致，各项一次真正的完整读取；不以两次任意 MRC 拼值或低位补零冒充成功 | SKIPPED |
| TIMER-H09 | CNTPCT/CNTVCT 在独立记录 debug freeze、频率与偏移的条件下采样，含受控回绕边界 | 模 2^64 差值及允许时窗一致；冻结不虚报必然进展；分别记录采样时间，不推导未经证明的原子快照或精确偏移 | SKIPPED |
| TIMER-H10 | 两核建立不同稳定值；Scope All 选 core0/core1 逐项读取，保存 peer 控制、选择器、暂存寄存器与 target | 只访问选定物理核，owner 与路由一致；target/暂存状态恢复，peer 控制及客户配置不变；旧 context 不跨核复用 | SKIPPED |
| TIMER-H11 | 有有效值后在隔离后端注入权限拒绝、缺少 MRRC、截断六十四位、断连与取消；另行验证恢复结果未知 | 最新原因与原值/依据分开；未知原因不猜测缺失；截断不 Valid；其他合法项继续，取消不发布新值；恢复未知进入 FAULT 并需重连 | SKIPPED |
| TIMER-H12 | 核对实际 GDB target description 六十四位 Timer 声明与值；PowerShell/VS Code 宽窄终端查看字段、完整高位、条件和长说明 | 名称可见与真实可读分开记录；全部字段说明可查看，进制及高位完整；查看不触发 Probe/read；不据本项单独勾选 REG-208 或完整后端验收 | SKIPPED |
| TIMER-H13 | 使用源码锁指定的新 Timer 候选及独立协议；核对 debug AP 的 MIDR、EDSCR.EL/RW/HDD、停止 DSPSR/DLR，分别覆盖实际 EL2、EL1/HDD=0/1 与 EL0；低 EL 未知场景不注入 Timer 指令 | EL2 十五项及 EL1 六项在合法条件下有新鲜证据；EL1 物理项保持权限未知，Hyp-only/EL0 必须拒绝的项明确受限；不以停止 CPSR、旧 Probe 或配置猜测当前 EL；前后完整状态与 PC 保持、R0/R1 恢复回读；case 的 require_timer_adapter=true 将每次值和物理证据写入报告，无旧通道回退 | SKIPPED |
| TIMER-H14 | 使用独立固件基线与新协议，以实际所选核执行低字自然进位、允许冻结/要求进展、超出记录窗口场景；审核过的虚拟偏移初始化可建立 CNTVCT 回绕边界；保存完整值、单项传输方式及各次主机请求区间 | 六项 64 位各一次 MRRC；Rt 低字/Rt2 高字，回读期间计数变化不撕裂单次值；模 2^64 差值按独立窗口核对，冻结不误判必然进展，旧基线/异常高字/倒退有失败原因；主机区间仅是当前核工作会话的传输界限，不推导跨条目/跨核同时性或精确 CNTVOFF；没有合法构造物理 64 位回绕的板级条件时记录该子场景 SKIPPED，调试器不改计数器/偏移/模式 | SKIPPED |


已有 `scripts/test-register-timer-hardware.cjs` 默认只生成五项 SKIPPED 报告，不连接目标。`REG-H05` 示例已覆盖四个 CVAL/offset 的独立符号、两个计数器及保持检查；先核对专用正常执行 Hyp 固件的模式与 ready，错误模式或未就绪不继续访问。九项 MRC/六项 MRRC 固件样本按每核独立保存，动态 TVAL 及禁用位不能作为稳定期待值。实际使用前替换所有软件期待值，并保存正常执行与 Debug state 的区别：EDSCR.HDD/Hyp invasive debug 授权不能由停止前 CPSR 推断。以上完整权限及跨时间场景仍需逐项留证，不能把六阶段软件驱动通过换算成十四项上板 PASS。未实现或缺少环境时记录原因并保持 SKIPPED。

新 Timer 后端的已实现权限范围以上述 TIMER-H13 为准；TIMER-H03/H04 的完整 EL1 物理及 EL0 enable 适配仍未完成，不能在当前实现下要求其成功。专用 Hyp 基线示例默认 `require_timer_adapter=true`；仅用于验证旧 MRRC 时可在隔离 case 中显式设为 false。该基线要求实际停止 Hyp 和当前 Debug EL2，通用应用仍分别判断两种状态。
