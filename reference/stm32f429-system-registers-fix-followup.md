# STM32F429 系统寄存器专项修复与验收

本轮17项确认发现均已处理：15项确定缺陷修复，2项说明已修正并保留精确资料限制。原31项重新核对为修复成立21项、修复成立且保留合理资料限制10项；尚未解决的确定缺陷0项。最终Goal完成审计见本轮独立验收文件。

范围限定为F429目录及自动读取资格实现。R52目标文件和四个分片、公共M4/Armv7-M父目录、原审计/修复/复审报告及三个原测试文件哈希保持。R52仅做共享实现兼容和完整加载回归，原R52发现（包括REAUDIT-032）未在本轮修复。

修复前基线：`823509b83e71fd0d4c44b312c8c36571bdabaece`，已由前置任务提交并推送。修复结果保存在当前工作树；本轮未发布、未安装到其他环境、未连接硬件。

## 修改与完整范围

当前F429为617个定义、1464个读取/公共位域及32个独立写载荷位域；仍只有R0～R12、SP/LR/PC共16个执行writer。本轮468个定义、26个位域元数据/描述发生变化，没有新增或删除寄存器/字段，没有改变位域位置、reader/writer绑定、scope、条件及访问规则。
四个专用寄存器新增架构reset=0；两个WO保留位取消错误RO覆盖；FPRound删除未经本版本表支持的普通编码0。其余变化为来源章节及有原始依据的详细规则/限定说明。

PPB规则覆盖原348项及6个MPU别名共354项；MPU配置屏障覆盖10个设置视图；勘误覆盖29个原目标及3个RASR别名共32项；ROM版本定位覆盖56项；六个ETM ResourceA/B字段和五个DWT guard字段逐实例落实。完整字段差异与实例列表在JSON台账中。

## 17项确认发现逐项处置

| 编号 | 结果 | 确认发现 |
|---|---|---|
| REAUDIT-001 | 已修复 | 两个WO集成接口的保留位被结构化标为RO |
| REAUDIT-002 | 已修复 | 目录自动读取判定遗漏description_unsupported排除 |
| REAUDIT-003 | 已修复 | CONTROL和FAULTMASK具体写入前提仍不完整 |
| REAUDIT-004 | 已修复 | 22个核心/专用寄存器的来源章节名称与原文不符 |
| REAUDIT-005 | 已修复 | 四个专用寄存器遗漏已证实的架构复位零说明 |
| REAUDIT-006 | 已修复 | MVFR0.FPRound为编码0增加了不适用的普通能力含义 |
| REAUDIT-007 | 已修复 | DHCSR遗漏运行态禁止修改STEP/MASKINTS并丢失初始使能例外 |
| REAUDIT-008 | 已修复 | SHCSR未明确使能后的读改写和active保存恢复步骤 |
| REAUDIT-009 | 已修复 | 348个旧PPB入口仍遗漏非特权访问的BusFault结果 |
| REAUDIT-010 | 已修复 | 故障地址视图仍遗漏先保存地址、再检查有效标志的必要顺序 |
| REAUDIT-011 | 已修复 | MPU说明遗漏有未完成传输时配置前屏障的必要条件 |
| REAUDIT-012 | 已修复 | 29个受影响条目的勘误概述仍遗漏精确触发条件 |
| REAUDIT-013 | 说明已修正，合理限制保留 | 身份及ROM共用说明未区分多4KB块定位规则的来源版本 |
| REAUDIT-014 | 已修复 | 三个事件寄存器的ResourceA/B遗漏已公开M4资源编码 |
| REAUDIT-015 | 已修复 | ETMTRACEIDR遗漏单一跟踪源仍须配置的产品要求 |
| REAUDIT-016 | 已修复 | EmbeddedICE默认Pulse/Latch行为仍用泛化说明替代 |
| REAUDIT-041 | 说明已修正，合理限制保留 | 五个DWT功能字段遗漏capability guard的同版资料冲突 |

### REAUDIT-001 两个WO集成接口的保留位被结构化标为RO

两个Reserved位域取消RO覆盖，继承WO；IHI0014Q 3.1补证保留位写非零UNP，要求有效整字写时写0；无读回0/WI/固定reset推断。

覆盖2个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：etm_m4物理PDF32；etm_m4物理PDF43；etm_m4物理PDF44；etmv3_q物理PDF82。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[etm.itatbctr0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:12315)。其他实例定位见JSON。

### REAUDIT-002 目录自动读取判定遗漏description_unsupported排除

目录自动资格改为逐alias依赖调用寄存器reader/effect策略；未知、排除、满足和非法presence及无条件缺reader均拒绝；矩阵与session无IO拒绝已有实际回归。

覆盖1个当前视图；完整范围和修复前行为在JSON台账中。

当前定义：[etm.lar](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:12602)。其他实例定位见JSON。

### REAUDIT-003 CONTROL和FAULTMASK具体写入前提仍不完整

CONTROL的Handler SPSEL读0/软件显式写忽略明确；FAULTMASK的MSR优先级限制与debugger始终可置1的C1.6.4例外分别限定。

覆盖2个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：pm0214物理PDF24；pm0214物理PDF25；armv7m物理PDF519；armv7m物理PDF520；armv7m物理PDF706。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[control](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:8158)。其他实例定位见JSON。

### REAUDIT-004 22个核心/专用寄存器的来源章节名称与原文不符

全部22个核心/专用寄存器按B1.4.1、B1.4.3、B1.4.4角色修正section；页码及FP寄存器70项来源保留并核对。

覆盖22个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：armv7m物理PDF516；armv7m物理PDF519；armv7m物理PDF520。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[basepri](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:8121)。其他实例定位见JSON。

### REAUDIT-005 四个专用寄存器遗漏已证实的架构复位零说明

四个专用寄存器增加整数reset=0，限定架构处理器复位，不冒充板级线路或当前读值。

覆盖4个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：armv7m物理PDF519；armv7m物理PDF520。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[basepri](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:8121)。其他实例定位见JSON。

### REAUDIT-006 MVFR0.FPRound为编码0增加了不适用的普通能力含义

FPRound删除未获该Armv7-M表支持的普通编码0含义，保留唯一公开编码1及MVFR1联合解释。

覆盖1个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：armv7m物理PDF662。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[fpu.mvfr0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:1089)。其他实例定位见JSON。

### REAUDIT-007 DHCSR遗漏运行态禁止修改STEP/MASKINTS并丢失初始使能例外

DHCSR整字和四个控制字段补充Non-Debug修改禁例、TableC1-9组合及初始DEBUGEN=0/MASKINTS写0例外；旧无例外绝对规则被替换。

覆盖1个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：armv7m物理PDF695；armv7m物理PDF696；armv7m物理PDF700。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[dcb.dhcsr](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:983)。其他实例定位见JSON。

### REAUDIT-008 SHCSR未明确使能后的读改写和active保存恢复步骤

SHCSR明确active保存/恢复及堆栈一致性、系统handler使能后必须RMW，只改所需位；准确引用PM0214 4.4.9。

覆盖1个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：pm0214物理PDF219。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[scb.shcsr](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:171)。其他实例定位见JSON。

### REAUDIT-009 348个旧PPB入口仍遗漏非特权访问的BusFault结果

354个相同PPB共用规则成员全部补默认非特权BusFault，含原348项及6个MPU别名；STIR与ITM自身例外、DAP访问另行限定。

覆盖354个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：armv7m物理PDF594；m4_r0p1物理PDF51；m4_r0p1物理PDF52；m4_r0p1物理PDF85；armv7m物理PDF686。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[dwt.comp0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:7275)。其他实例定位见JSON。

### REAUDIT-010 故障地址视图仍遗漏先保存地址、再检查有效标志的必要顺序

两个地址视图及两个CFSR有效位均写清先保存地址再读VALID与高优先级抢占原因；保留共享存储及有效标志互斥，不声称跨寄存器原子读取。

覆盖3个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：pm0214物理PDF226；pm0214物理PDF227；m4_r0p1物理PDF51。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[scb.bfar](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:278)。其他实例定位见JSON。

### REAUDIT-011 MPU说明遗漏有未完成传输时配置前屏障的必要条件

全部10个MPU设置视图补配置前未完成传输屏障、配置后新设置同步、PPB设置期间及异常进入/返回例外。

覆盖10个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：pm0214物理PDF181；pm0214物理PDF182；pm0214物理PDF183。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[mpu.ctrl](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:413)。其他实例定位见JSON。

### REAUDIT-012 29个受影响条目的勘误概述仍遗漏精确触发条件

29个原目标及3个RASR别名共32个勘误说明补齐14周期/ISR指令数/外部栈，以及A/B/C中断、bufferable immediate store、BX和等待条件；其余产品勘误限制保留。

覆盖32个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：es0206_r24物理PDF1；es0206_r24物理PDF2；es0206_r24物理PDF7；es0206_r24物理PDF8；es0206_r24物理PDF9；es0206_r24物理PDF11。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[d0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:8178)。其他实例定位见JSON。

### REAUDIT-013 身份及ROM共用说明未区分多4KB块定位规则的来源版本

全部56个身份/ROM视图将无条件最低块断言改为DDI0314H最低块和IHI0029D D5.2.5 TableD5-2最高管理块的版本限定；单4KB现有地址保持。

覆盖56个当前视图；完整范围和修复前行为在JSON台账中。

保留限制：扩展多4KB组件及实际适用集成版本未观测；现有单4KB基线地址不受版本定位差异影响。

原始依据：coresight_h物理PDF105；coresight_d物理PDF140；coresight_d物理PDF141。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[dwt.cid0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:9866)。其他实例定位见JSON。

### REAUDIT-014 三个事件寄存器的ResourceA/B遗漏已公开M4资源编码

三个事件的六个ResourceA/B字段具备完整M4编码20..23/40/5F/60/61/6F；无效资源UNKNOWN/事件UNP及单资源函数忽略B明确。

覆盖3个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：etm_m4物理PDF23；etm_m4物理PDF24；etm_m4物理PDF25；etmv3_q物理PDF194；etmv3_q物理PDF195；etmv3_q物理PDF196；etmv3_q物理PDF197；etmv3_q物理PDF198。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[etm.teevr](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:12359)。其他实例定位见JSON。

### REAUDIT-015 ETMTRACEIDR遗漏单一跟踪源仍须配置的产品要求

TRACEIDR整字及TraceID字段增加即使单源也必须配置、多源唯一；7bit、ETMreset0和平台分配未知分开。

覆盖1个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：etm_m4物理PDF22；etmv3_q物理PDF163；etmv3_q物理PDF164。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[etm.traceidr](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:12493)。其他实例定位见JSON。

### REAUDIT-016 EmbeddedICE默认Pulse/Latch行为仍用泛化说明替代

四个相关视图和CCER能力位明确Trigger/StartStop为Pulse、TraceEnable为Latch及采样保持边界；无EIBCR入口，不外推Timestamp行为。

覆盖4个当前视图；完整范围和修复前行为在JSON台账中。

原始依据：etm_m4物理PDF40；etm_m4物理PDF41；etmv3_q物理PDF162；etmv3_q物理PDF346；etmv3_q物理PDF347。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[etm.ccer](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:12171)。其他实例定位见JSON。

### REAUDIT-041 五个DWT功能字段遗漏capability guard的同版资料冲突

四个FUNCTION视图、五个准确字段记录同版capability guard冲突；保留已确认位号、reset、功能及支持极性，冲突分支精确读回/写响应仍缺正式更正。

覆盖4个当前视图；完整范围和修复前行为在JSON台账中。

保留限制：无适用正式更正，五字段冲突guard分支的精确读回/写响应依据不足；保留已确认功能/能力极性，不宣称硬件证伪。

原始依据：armv7m物理PDF719；armv7m物理PDF721；armv7m物理PDF725；armv7m物理PDF730；armv7m物理PDF747；armv7m物理PDF748；m4_r0p1物理PDF85；m4_r0p1物理PDF86。文档路径、版本、PDF/文本SHA256及章节定位见证据台账。

当前定义：[dwt.function0](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers.toml:7428)。其他实例定位见JSON。

## 原31项问题重新核对

| 编号 | 当前结论 | 当前语义核验 |
|---|---|---|
| F429-001 | 修复成立 | DISMCYCINT[0] RW且禁止多周期中断会增加延迟，吻合r0p1 Table4-2；本轮仅扩充条件勘误和PPB规则。 |
| F429-002 | 修复成立 | 全部70个core/FP条目来源非空且角色适配；22个错误章节已修正；完整本地override保留reader/writer和真实继承来源。 |
| F429-003 | 修复成立 | 六个专用寄存器布局、word alignment、屏蔽/阈值及FPCA/nPRIV保留，补齐Handler SPSEL、FAULTMASK访问路径和四reset0。 |
| F429-004 | 修复成立 | 独立解析ST官方header，280个NVIC实例全查地址/完整位域/条件及优先级，手册W1S/W1C/RO语义核对，ICTR仍只是bank上界。 |
| F429-005 | 修复成立 | STIR E000EF00/WO/INTID[8:0]及CCR.USERSETMPEND例外保留，description_only无执行writer；不沿用PM的SCR笔误。 |
| F429-006 | 修复成立 | 六个MPU别名地址按+8/+16/+24核对；A编号不选择region，规则同所选寄存器；本轮共用屏障/PPB说明涵盖别名。 |
| F429-007 | 修复成立 | ACTLR DISOOFP9/DISFPCA8及SBZP准确，采用r0p1而非r0p0相反位图；CPUID文档期望与实际读数仍分开。 |
| F429-008 | 修复成立，合理限制保留 | AFSR单周期输入锁存/W1C/写0保持/不触发异常的规则保留；AUXFAULT集成及复位资料差异仍有明确限制。 |
| F429-009 | 修复成立 | 八区域、RNR0..7、SIZE4..31、SRD及完整AP/TEX/C/B/S语义核对；配置前后和异常边界屏障现已补齐。 |
| F429-010 | 修复成立，合理限制保留 | 全部多级能力字段与组合依赖保留，删除FPRound无效普通编码0；MVFR2目标实现仍低可信且默认条件Unknown。 |
| F429-011 | 修复成立 | ICSR互斥命令、CPACR相同权限/DSB/ISB、MON_STEP优先级、POSTCNT两步和FPCCR更改前提保留；DHCSR/SHCSR缺项已补。 |
| F429-012 | 修复成立 | 原348项和共享规则额外6别名全部具备程序访问BusFault说明，未加入EL限制或混淆DAP授权。 |
| F429-013 | 修复成立 | DCRSR/DCRDR布局及Debug/REGSEL/S_REGRDY传输顺序保留；DCRSR WO无reader，DCRDR reset未填0，无执行写新增。 |
| F429-015 | 修复成立 | TENMS18750仅对应HCLK150MHz、参考18.75MHz下1ms；当前读值和SKEW/NOREF仍未拼整字reset。 |
| F429-016 | 修复成立，合理限制保留 | MVFR2架构定义与目标实现分离，low可信度和presence条件保留，实际测试默认自动候选排除。 |
| F429-017 | 修复成立 | 90个命名IRQ、79空洞、第三bank07FF7FFF及六个reserved priority byte均独立核对，generic高bank不作产品IRQ证明。 |
| F429-018 | 修复成立，合理限制保留 | SP栈选择/低两位和writer地址约束、LR返回/FFFFFFFF、PC DebugReturnAddress/bit0与EPSR.T区分保留；实际GDB传输行为仍未硬件证实。 |
| F429-019 | 修复成立 | 两访问地址、共享物理存储、VALID互斥保持，现补地址→VALID读取次序；不当成两个独立物理存储。 |
| F429-020 | 修复成立 | 28个DWT/FPB定义的字段说明均非空，六个32bit整字字段保留；本轮五个guard字段限定补齐而未删结构。 |
| F429-021 | 修复成立 | RMPSPT为RO能力，REMAP受能力条件及SRAM地址重建/对齐约束，不继承整字RW为可配置能力。 |
| F429-022 | 修复成立 | CYCCNT及四个FUNCTION reset0保留，八FP comparator仅ENABLE POR清0而整字reset未伪填；041的额外guard冲突另列，不推翻本项复位修复。 |
| F429-023 | 修复成立 | 四组COMP/MASK的0..15/最大32KB、低N位清0、指令半字及cycle/data MASK0均保留，不执行写31探测。 |
| F429-024 | 修复成立 | 八个比较器及REMAP的完整地址/至少8word对齐/非bit-band、双enable、REPLACE/literal区别和原地址MPU规则保留。 |
| F429-025 | 修复成立，合理限制保留 | 全部计数器32/8bit回绕、使能和halt规则保留；PCSR自读/TRCENA/Debug sentinel及用DHCSR确认halt明确，可选PCSR集成仍未知。 |
| F429-026 | 修复成立，合理限制保留 | 四个FUNCTION的三模式编码、SBZ/link编号和自身/第二链接规则保持，MATCHED读清；1011/1111和DATAVSIZE3同版规范/伪码冲突限制保留。 |
| F429-028 | 修复成立，合理限制保留 | 当前ITM32端口/49视图、TPIU28视图（原26接口及FFSR/FFCR两个formatter接口）及ETM40视图均保留，无真实writer；WO/unsupported资格、资源映射、ID初始化及默认ICE行为已补，产品集成/锁等限制保留。 |
| F429-029 | 修复成立，合理限制保留 | 四DBGMCU条目32bit，ID RO、控制RW/POR0/SYSRESET保持、chip owner严格映射；原图/复制/时钟差异保留，具体芯片修订和route仍未知。 |
| F429-030 | 修复成立 | 全部29原目标和3别名的条件勘误已补完整触发时序，保留Stop/SysTick及RTC/IWDG条件，不改变硬件配置。 |
| F429-027 | 修复成立，合理限制保留 | 53个原缺项及56个现有身份/ROM视图全保留，PRESENT/FORMAT/signed offset/end0/MEMTYPE UNKNOWN和版本定位限定明确，SoC替换/候选ID及多块集成未假定。 |
| F429-031 | 修复成立，合理限制保留 | FFSR E0040300 RO/8及FFCR E0040304 RW/102、十分区及门控/同步/SWO资格保留，无writer，文档常数不作本板观测。 |
| F429-032 | 修复成立 | FPSCR FZ输入/tiny-before-rounding/IDC-UFC、DN算术范围、V比较unordered与OFC算术overflow分离，PM prose/Table57限制保留。 |

原F429-014是已解除历史记录，未重复计入；F429-032属于有效31项。原F429-022的复位修复成立，REAUDIT-041对新增/扩展功能字段的guard资料冲突单独保留，不混淆原问题和新增发现计数。

合理限制包括MVFR2/PCSR可选集成、AFSR输入/复位资料差异、实际GDB写入语义、DWT规范/伪码冲突、ETM/ITM/TPIU版本电源认证及锁集成、ROM扩展/SoC替换、DBGMCU芯片修订与owner路由。配置声明、文档值和离线规划均不冒充硬件观测。

## 离线验证与新版EXE

- 修复前原复审测试：4通过/2失败，原断言和失败日志保留。
- 修复后原独立复审测试：6通过/0失败；原修复回归18通过；运行时回归7通过。
- 新增及相关unsupported行为回归：6通过，549过滤；原三份测试文件未修改。
- registers库回归：231通过、1硬件回放忽略、323过滤。忽略/过滤不计通过。
- Cargo回归使用test-and-clean.ps1及--locked --offline；因后续仍使用target中的产物，临时DEBUGTUI_KEEP_TEST_PAYLOADS=1，最终统一清理。
- 使用仓库build.ps1 -Release并设置CARGO_NET_OFFLINE=true构建，退出0。
- 实际release EXE的8个CLI用例全部退出0：F429未知/已配置ETM存在、R52参考目录、五种内置CPU目录。请求只有registers_list/registers_matrix/quit，无mi>/tcl>命令或观察值。
- 两种F429配置的全部617个定义、所有字段/枚举/权限/规则/来源/完整继承及override来源逐项与独立源展开一致；R52全部2244个定义作相同完整加载回归。ETMLAR两种情况下都保留架构可读元数据且无自动资格/可用读取计划。

保留的本地新版EXE：[debugtui.exe](G:/Data/GitFiles/DebugTUI/bin/debugtui.exe)，版本`debugtui 1.1.4`，15,614,464字节。
SHA256：`42d4cedbf29256f9039f66a44371063fcab6efc7f95e50c13f2bf7848fa89b7c`。已通过原release路径完整验证，复制到bin后版本和哈希再次核对。
版本号沿用1.1.4，这是一份本地重新构建的修复EXE，以SHA256区分；没有生成或发布新Release。
原bin EXE按原哈希单独保留：[debugtui-pre-f429-followup-a4407118231e.exe](G:/Data/GitFiles/DebugTUI/bin/debugtui-pre-f429-followup-a4407118231e.exe)；历史附件/源码/会话及证据均保留。

F429产品说明位于外部reference目录。实际项目应使用[registers].catalogue指向本轮stm32f429-system-registers.toml；仅选择内置cortex-m4仍使用通用父目录。

## 清理与证据保护

已执行`pwsh -NoProfile -File scripts/cleanup-build.ps1 -Apply`，退出0。target前3,692,127,724字节，后0字节，本轮实际释放3,692,127,724字节（3.44 GiB）。
没有Node分发/打包轮次、没有新增npm cache或staging；未清理其他失败/硬件轮次，未移动产物到其他磁盘。报告、原始PDF、原始测试及失败日志、CLI gzip输出、基线、正式EXE、工具链、用户配置、会话及.fixture-store/旧清单保留。
本轮发现并修正的脚本/测试诊断（复位类型、新测试诊断provenance预期、精确章节定位）单独记录；原失败记录未覆盖、旧断言未改。

## 可复核产物

- [完整修复台账JSON](G:/Data/GitFiles/DebugTUI/reference/stm32f429-system-registers-fix-followup.json)
- [17项处置、31项语义复核及完整变更范围](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/semantic-review.json)
- [9份原始PDF/265个直接抽取页的证据清单](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/primary-page-manifest.json)
- [全部280个NVIC视图与官方header核对](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/nvic-current-source-check.json)
- [实际新版EXE完整加载验证](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/native-cli-verification.json)
- [已测试EXE版本及哈希](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/tested-executable.json)
- [清理精确字节记录](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/final-cleanup.json)
- [逐要求完成审计](G:/Data/GitFiles/DebugTUI/artifacts/system-registers-fix-followup-f429/goal-completion-audit.json)
