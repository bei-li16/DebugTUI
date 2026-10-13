# R52 / STM32F429 系统寄存器修复复审

**复审工作完成。全部212项、307个来源单位/320个变体、新增及受影响范围已复核；离线验证、最终报告一致性和清理完成。当前修复结果不通过全部修复验收。**

原问题状态：修复成立 148项；确定部分修复成立，合理资料限制保留 40项；部分修复，仍有确定缺陷 24项；未真正修复 0项；修复引入回归 0项；证据不足，无法验证关闭结论 0项。未复审原问题0项。

当前确认发现41项。F429原31项为修复成立14、合理限制7、部分修复10；R52原181项为修复成立134、合理限制33、部分修复14。F429-014是已解除的历史记录，不重复计数；F429-032包含在有效原问题中。新发现与原问题数量分别统计；原要求已修复但新增/扩展字段说明另有缺口时，保留原问题的具体结论，另列新增发现。

## 覆盖、继承与实际加载

R52根文件725个定义，实际继承展开2244个描述及21501个读取/公共位域；根加四个GIC分片共5节点、15030711字节。F429实际展开617个描述、1464个读取/公共位域及32个独立写载荷位域，共1496个位域；继承图3节点。原1156个reader/writer绑定保持；R52无writer，F429仍为R0～R12、SP/LR/PC共16个，独立写载荷不增加执行writer。

全部1519个新增GIC视图和17752个位域逐实例与独立原始公式比较：Distributor1485个/17330字段，当前物理核Redistributor33个/408字段，可选导出TYPER1个/14字段。28个ROM入口均为cluster，GICD及导出身份为cluster、物理核GICR为对应core，真实映射缺失时拒绝借用其他owner或通用基址。结构化条件为ITLinesNumber1..30、物理核数1..4及实际导出事实；未知/非法条件不会默认最大值，固定15个Distributor发现入口不依赖未知数组容量而数组保留Unknown/No，这不是最大实例读取授权。

描述视图、数组槽位与物理地址分开：已配置合法条件下GIC目录槽位公式15+49*N+33*C+E；N为容量1..30，C为物理核1..4，E为可选导出身份0/1。当前核模板按显式owner重复使用；公式不是实际硬件地址数量，真实PERIPHBASE、核心/导出及总线通路均未观察。导出端口不能当额外处理器，176个PPI字段明确排除，其他217个导出字段适用性仍不足。Source SRC173..176的产品/架构配置、ICPENDR、ICFGR1及priority未用位冲突保留。

真实CLI对两份参考目录的全部2861个最终定义逐项比较，包括所有字段/枚举、结构权限、说明、条件、缺省值、来源、完整继承路径与覆盖来源。四分片未丢失定义或隐式覆盖。原始TOML省略Scope/Condition.max与CLI序列化缺省值的差异仅在独立比对工具按真实serde定义规范化，被审输入没有改变。

## 资料限制与完成范围

修复报告的52项保留限制声明全部与独立原始资料复审对应：32项确定部分成立并保留精确限制；14项为规范已定义的UNKNOWN或条件/CU行为，原具体缺陷修复成立；6项仍有已确认缺陷，不以资料不足关闭。详细见claimed-limits-52-reaudit.json。

原57条直接证据不足记录已全部独立复核（57/57）。保留明确的版本、值、权限和条件边界；不把“当前TOML限定说明不足”宣称为“真实硬件值已证伪”。新增033至041分别记录未准确限定的字段/访问规则。来源判断已有307/307个单位、320个历史变体完成独立协调，剩余0个单位。已用后续原始资料补证的DBGMCU位图/通用CLAIM-LSR来源缺口，不继续充当当前缺证理由，实际芯片/认证/电源/版本限制仍保留。来源协调、历史导航及检查覆盖成员关系均不代替完整新增/受影响字段和共用规则的语义完成审查。

所有原问题已有结论、已有测试通过或输入哈希一致均不能代替完整语义核验。原文页抽取也不等于完整语义已验证；当前直接抽取18份适用本地PDF、3727个物理页。资料确实不足时保留有依据的限制，不要求硬件操作，不无限检索。

HSR已完整核对17个EC、9种ISS格式及60个条件字段解释，逐格式覆盖低25位；IL有效性、指令方向、条件码、fault有效域、IFSC/DFSC与读回生命周期均完成原始资料对照。当前3个结构字段不制造重叠字段，条件子布局在ISS详细说明；既有TA/coproc同版正文/伪码冲突023继续列为确定说明缺陷。独立修复前后差异清单发现R52新增1519个视图、1993个受影响定义，F429新增186个视图、612个受影响定义；其中R52新增17769个字段、改498个、移除1个，F429新增1162个读取/公共字段、改88个、另增32个独立写载荷字段。字段变更计数与最终字段总量不同，完整范围见turn11-affected-definition-independent-inventory.json。全部2605个受影响定义及19550个字段变更/共用规则已完成语义复核，具体实例和失败/限定结论见turn12-complete-affected-semantic-scope-review.json；目标247条非空文本要求逐条证据见turn12-goal-requirements-audit.json。

## 当前已确认发现

### [P2] REAUDIT-001 两个WO集成接口的保留位被结构化标为RO

关联原问题：F429-028；寄存器：etm.ittrigout, etm.itatbctr0。

字段属性在UI显示为RO，但整个接口为WO且原表只规定Reserved，没有独立可读位的依据。description_only仍拒绝实际读取，本项未发现真实I/O扩展。

建议：保留Reserved说明并避免为其赋予可读权限；例如移除不当字段access使其继承WO，再核对保留位写规则。不能把Reserved推导成RAZ/WI。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12269
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12311

### [P2] REAUDIT-002 目录自动读取判定遗漏description_unsupported排除

关联原问题：F429-028；寄存器：etm.lar。

配置存在条件满足后，UI使用的目录级自动候选判定与寄存器级判定不一致，可能自动请求本应跳过的入口。session提前返回ReaderUnsupported，实际读取通路仍无I/O；本项为资格判定/状态显示回归。

建议：在目录自动读取判定中包含依赖reader的description_unsupported排除，验证alias依赖不绕过限制，并检查矩阵自动资格。保留无I/O分支，不增加读取实现。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12598

### [P2] REAUDIT-003 CONTROL和FAULTMASK具体写入前提仍不完整

关联原问题：F429-003；寄存器：control, faultmask。

已经补齐布局和大多数作用，但关键写入前提仍是泛泛说明，不能据此关闭“具体使用条件”全部缺陷。

建议：按处理器软件MSR访问路径明确Handler的SPSEL读0/显式软件写忽略，以及执行优先级-1或更高时MSR不能将FAULTMASK置1，HardFault/NMI handler属于此范围。与DCRSR/DCRDR调试转移分开：DDI0403E.e物理706明确debugger可始终把FAULTMASK置1，但可能影响退出Debug后的行为，不能把MSR条件扩为所有调试写路径。保留本目录无FAULTMASK writer现状。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:8155
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:8138

### [P2] REAUDIT-005 四个专用寄存器遗漏已证实的架构复位零说明

关联原问题：F429-003；寄存器：primask, basepri, faultmask, control。

修复补充了位域，却没有保留原始资料明确的架构复位信息；缺省reset不能代表UNKNOWN或缺证。

建议：补充有来源的架构reset=0或明确等价复位说明，区分处理器架构复位与未知板级复位线路，不冒充当前读数。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:8103
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:8120
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:8138

### [P2] REAUDIT-006 MVFR0.FPRound为编码0增加了不适用的普通能力含义

关联原问题：F429-010；寄存器：fpu.mvfr0。

适用Armv7-M来源只定义1为全部舍入模式，当前新增0=不支持全部舍入模式没有本目标适用依据；不能用A/R实现定义范围替代M定义，也不能以整个FP扩展不存在解释为正常有效MVFR0字段。

建议：删除未经本目标资料支持的普通编码0含义，按适用架构明确未定义/保留范围；保留1及与MVFR1联合解释，避免断言未知硬件读数。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:1089

### [P2] REAUDIT-007 DHCSR遗漏运行态禁止修改STEP/MASKINTS并丢失初始使能例外

关联原问题：F429-011；寄存器：dcb.dhcsr。

C_STEP只有泛化合法组合说明，未明确C_DEBUGEN=1且S_HALT=0时改变STEP/MASKINTS是UNPREDICTABLE；无条件要求改变MASKINTS前后HALT=1还丢失原文“写前DEBUGEN=0且本次MASKINTS=0”例外。

建议：明确运行态禁改规则、单步退出Debug的HALT/STEP组合以及MASKINTS前后HALT要求的架构例外；保留DEBUGEN 0→1时MASKINTS=0、DBGKEY和SNAPSTALL复位规则。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:983

### [P2] REAUDIT-008 SHCSR未明确使能后的读改写和active保存恢复步骤

关联原问题：F429-011；寄存器：scb.shcsr。

虽然写了堆栈一致和保留其他状态，仍未明确原文要求的保存并随后恢复当前active状态，以及系统handler使能后改位必须RMW。此寄存器不能用泛泛“按原始规则”代替可执行步骤。

建议：补充原文限定的保存/恢复active状态和使能后RMW要求，区别于ICSR等混合命令位禁止普通RMW的规则。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:171

### [P2] REAUDIT-009 348个旧PPB入口仍遗漏非特权访问的BusFault结果

关联原问题：F429-012；寄存器：scb.cpuid, scb.icsr, scb.vtor, scb.aircr, scb.scr, scb.ccr, scb.shcsr, scb.cfsr, scb.hfsr, scb.dfsr, scb.mmfar, scb.bfar（共348项，完整范围见JSON）。

全部348条都补了特权要求，却没有把非特权PPB访问引发BusFault写出；原问题的明确故障类型要求尚未完成。STIR和ITM等例外必须按各自来源保留。

建议：在适用条目或可追溯共用说明中补明确BusFault结果及调试资源例外，不给所有新ITM端口一概套此规则，不将DAP授权规则与CPU程序访问混淆。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:37
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:59
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:86

### [P2] REAUDIT-010 故障地址视图仍遗漏先保存地址、再检查有效标志的必要顺序

关联原问题：F429-019；寄存器：scb.mmfar, scb.bfar。

共享存储及互斥标志已正确补齐，但“同一上下文一起解释”未写出原文必须先读并保存地址，再读取对应VALID位；较高优先级异常可能改写共享值。此为受影响定义的操作顺序缺项。

建议：补充两个访问视图的地址→VALID读取顺序及抢占原因；静态描述不应引入真实寄存器访问或自动轮询。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:260
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:278

### [P2] REAUDIT-011 MPU说明遗漏有未完成传输时配置前屏障的必要条件

关联原问题：F429-009；寄存器：mpu.ctrl, mpu.rbar, mpu.rasr。

禁用区域和配置后DSB/ISB已补，但当前说明只强调更新后同步，未说明可能有受新属性影响的未完成传输/缓冲写时，配置前也必须屏障，以及异常边界提供屏障的限定。

建议：补充配置前待完成传输条件及进入异常/异常返回的限定，保留PPB配置期间不必逐次插屏障与配置后同步的区别。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:413
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:455
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:478

### [P2] REAUDIT-012 29个受影响条目的勘误概述仍遗漏精确触发条件

关联原问题：F429-030；寄存器：scb.actlr, fpu.fpccr, fpscr, mpu.rasr, systick.ctrl, s0, s1, s2, s3, s4, s5, s6（共29项，完整范围见JSON）。

修复正确写出LSPEN/DSB及其他两类产品限定，但“极短ISR”和“相关store”概述没有保留全部精确触发条件，却声明已保留全部。读者可能把条件勘误泛化或误判适用性。

建议：在可追溯共用说明或各条目中补776924的14周期/ISR计数和外部栈例外，补838869的具体store/等待/BX/中断优先级时序；保留DISDEFWBUF仅无MPU及DSB的限定，不执行任何硬件改动。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:616
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:525
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:584

### [P2] REAUDIT-014 三个事件寄存器的ResourceA/B遗漏已公开M4资源编码

关联原问题：F429-028；寄存器：etm.trigger, etm.teevr, etm.tsevr。

布尔函数和7bit type/index结构正确，但当前只写资源必须按实现限定，未列DWT0..3=20..23、Start/Stop=5F、ExtIn0/1=60/61等已公开编码。引用“前一阶段完整384资源行”不是TOML本身的位域含义说明。

建议：在ResourceA/B或清晰适用共用说明列完整M4资源映射：20..23、40、5F、60/61按0..2集成条件、6F；其它不支持/保留资源按UNKNOWN读值和事件UNP处理，单资源函数忽略B。不要按通用最大目录添加资源。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12332
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12355
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12378

### [P2] REAUDIT-015 ETMTRACEIDR遗漏单一跟踪源仍须配置的产品要求

关联原问题：F429-028；寄存器：etm.traceidr。

当前仅描述多源同时跟踪时ID唯一，没有保留产品手册即使只有单一trace source也必须配置TRACEIDR的要求。产品reset0是复位状态，不能代替运行配置。

建议：明确单源也须配置TRACEIDR、多源须唯一，保留7bit/ETMreset0及平台ID分配未知限定；本工具继续无writer，不执行配置。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12489

### [P2] REAUDIT-016 EmbeddedICE默认Pulse/Latch行为仍用泛化说明替代

关联原问题：F429-028；寄存器：etm.ccer, etm.trigger, etm.teevr, etm.tesseicr。

已写EIBCR未实现、输入按默认行为，但没有说明默认Trigger与Start/Stop使用Pulse、TraceEnable使用Latch；同一DWT输入用于不同资源会有不同保持行为，不能仅凭“默认”知道具体含义。

建议：补充适用M4 EIBCR缺省规则及采样/保持边界，区分Trigger/StartStop的脉冲与TraceEnable的锁存。不要把不存在的EIBCR作为可配置入口，不给新增Timestamp资源臆定表中未规定行为。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12167
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12332
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:12355

### [P2] REAUDIT-020 Context ID/VMID旧新值窗口遗漏更新指令前的P0边界

关联原问题：R52-039；寄存器：trccidcvr0, trcvmidcvr0。

两个条目正确补了旧/新值及推测匹配，但将窗口概述从PE更新开始，遗漏规范的前一个P0锚点。用户可能把该块中更新之前的比较结果误当作必定使用旧标识。

建议：在两个条目明确窗口起点是标识更新系统指令之前的P0元素之后，终点是更新后上下文同步；保留Debug不匹配、EL过滤与PE输入0/期望值UNKNOWN的区别。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15111
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15128

### [P2] REAUDIT-022 HPRSELR遗漏同类EL1尝试访问的HSTR.T6陷入分支

关联原问题：R52-056；寄存器：hprselr。

原列出的15个入口已补陷入条件，但同类EL2 MPU选择器仍只列正常EL2权限；无法区分EL1 HSTR.T6=1时Hyp陷入与无陷入时UNDEFINED。TRM正常权限概要不能证明两种非法尝试有相同结果。未授予软件实际写入能力。

建议：在HPRSELR正常EL2 RW规则旁补EL0 UNDEFINED、EL1 HSTR.T6=1先陷入Hyp否则UNDEFINED，并保留TGE和调试授权范围；保持原编码、选择器同步、容量边界及无writer。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:3025

### [P2] REAUDIT-023 HSR EC=7 coproc含义未说明同一架构手册正文与伪代码的冲突

关联原问题：R52-078；寄存器：hsr。

0x34/0x35来源错误已修复，但同一ISS的EC7解码仍无条件采用正文。适用手册自身的生成伪代码对TA0给出相反低4位，当前说明无法支撑唯一解码结论。确认为资料冲突说明缺项；不冒称已证明实际硬件返回哪一分支，也不将其计为修复新引入的回归。

建议：在EC7 coproc说明中显式保留DDI0568A.c正文PDF124和伪代码PDF248的相反分支；如采用进一步适用原始规范消歧，列明版本、页码及优先依据，否则该低4位保留待消歧限制。TA、CV/COND和其他已确定布局不受此限制。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:3060

### [P2] REAUDIT-024 两个ICV组Enable位域仍将单项使能列为虚拟投递必要条件

关联原问题：R52-108；寄存器：icv_igrpen0, icv_igrpen1。

根描述已补globalEN、VENG别名和软件注入条件，但两个Enable位域保留物理组通用模板“还需中断单项enable”。LR虚拟注入没有相应独立单项enable字段，可以没有对应物理中断；字段说明与根说明矛盾，可能误导为必须使能物理中断才能投递虚拟中断。原正确的VENG和维护条件不因此失效。

建议：将两个ICV Enable字段的投递前提改为ICH_HCR.EN、对应VENG、适用pending LR以及priority/mask/preemption条件；保留物理关联模型的独立条件，不要求软件注入普遍存在物理单项enable。同步根与位域表述，维持RW/复位0和无writer。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:6702
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:6721

### [P2] REAUDIT-029 两个外部DCC Memory视图遗漏非字对齐访问后的UNKNOWN状态与数据

关联原问题：R52-144；寄存器：dbgdtrrx_el0, dbgdtrtx_el0。

产品选择非对齐事务并未取消基础H4.3.2.4的后续UNKNOWN边界。当前说明容易被用于在alignment checking关闭时继续依赖确定R0+4、已写数据或下一full状态，进而误判进度和重试。正常字对齐事务、R1失效、错误门控和产品不对齐访问选择已补齐；仅离线描述核验，无执行接口/硬件动作。

建议：将正常R0+=4和数据保证限定于适用的字对齐无错误事务；补齐非字对齐分支在无DataAbort后R0、传输数据和full的UNKNOWN结果，区分产品选定访问方式与基础后续状态。保留alignment checking时fault、DataAbort保留故障地址和错误重试边界，不能把UNKNOWN再改成确定增量。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9291
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9308

### [P3] REAUDIT-004 22个核心/专用寄存器的来源章节名称与原文不符

关联原问题：F429-002；寄存器：r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11（共22项，完整范围见JSON）。

已增加可定位的正确文档/物理页，但章节标签错误削弱精确来源对应；不否定这些页本身提供的定义证据。

建议：修正section为实际角色对应章节；PC调试返回语义可另列C1.6.3/C1.6.4来源，不用PSR章节代称所有核心寄存器。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7836
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7850
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7864

### [P3] REAUDIT-013 身份及ROM共用说明未区分多4KB块定位规则的来源版本

关联原问题：F429-027；寄存器：scs.pid4, scs.pid5, scs.pid6, scs.pid7, scs.pid0, scs.pid1, scs.pid2, scs.pid3, scs.cid0, scs.cid1, scs.cid2, scs.cid3（共56项，完整范围见JSON）。

当前目标表所列组件为单4KB，未发现这6个静态目标地址错误；但不加版本的通用说明可能误导扩展ROM解析和多块组件定位。属于来源适用范围说明缺陷。

建议：明确DDI0314H旧DAP ROM规则与IHI0029D新CoreSight规则的版本/适用范围；目标单4KB基线与扩展多块组件分开，不能无条件承诺最低块，也不要自动修改reader地址。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:9211
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:9232
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:9252

### [P3] REAUDIT-017 FPSID合法写忽略规则与保留的只读模板互相矛盾

关联原问题：R52-006；寄存器：fpsid。

新增说明已准确区分RO内容与合法VMSR写忽略，但同条目仍声称没有允许的软件写入操作；对原问题的修复说明不自洽。未新增实际writer，不构成执行写入风险。

建议：删除或替换FPSID的旧RO通用句，明确特权合法VMSR被忽略、EL0 UNDEFINED、CPACR/HCPTR与TID0读陷入分别适用；保持access=ro和无writer。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15616

### [P3] REAUDIT-018 Sequencer中间状态事件的必须active规则被弱化为可active

关联原问题：R52-021；寄存器：trcseqevr0, trcseqevr1, trcseqevr2, trcseqrstevr。

reset优先、forward优先及多级迁移已修复，但规范对经过状态事件的强制行为仍表述成可选行为。使用sequencer触发其他事件时，描述无法清楚支撑必然触发的判断。没有实际writer变化。

建议：在全部4个条目把经过状态所连接事件改为该周期必须active，保留reset不激活中间状态例外；另述事件流水线可能造成观察时延，不把内部规则写成外部观察无延迟保证。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13649
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13672
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13695

### [P3] REAUDIT-019 PCREL和SPREL字段共享PC/SP模板，未各自限定基址

关联原问题：R52-035；寄存器：trcvdctlr。

寄存器整体的PC/SP集体说明正确，但两个独立字段都声称本字段影响以PC/SP为base的访问。PCREL仅控制PC-base、SPREL仅控制SP-base，当前通用模板留下范围歧义。编码和实际writer没有变化。

建议：PCREL字段只写PC为base加immediate offset；SPREL字段只写SP为base加immediate offset。保留SPREL01保留及10的P1链接条件，整体说明可继续使用PC/SP集合表述。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13585

### [P3] REAUDIT-021 ViewData过滤组合的必要条件表述未保留邻近传输例外

关联原问题：R52-034；寄存器：trcvdctlr, trcvdsacctlr, trcvdarcctlr。

正常过滤逻辑已补齐，但追加条款以无条件“才能输出”表述，未保留ViewData拒绝时、邻近另一个必须跟踪传输的架构允许额外跟踪情形。通用不精确说明未给出该具体允许条件；不能据当前表述断定拒绝过滤必然不产生任何传输记录。没有声明实际R52必定采用该允许行为。

建议：将AND关系限定为正常过滤判定，补架构对邻近必跟踪传输允许额外跟踪的情形、避免过度跟踪要求及TRM精度条件；仍保留parent instruction、地址和值部分各自限制，不泛化为任意额外地址/值输出。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13585
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13609
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:13629

### [P3] REAUDIT-025 ICV_DIR模式0资料冲突说明未列出相反的写忽略分支

关联原问题：R52-116；寄存器：icv_dir。

TOML仍直接选定模式0为UNPREDICTABLE，后文只笼统称资料冲突继续保留，未写同版专用DIR条款的写忽略结论及页码。独立原来源判断保存了双方，但当前寄存器读写规则本身未明确该不确定边界。确认为限制说明缺项；不宣称实际R52应当采用写忽略，也不否定正常mode1已修复的生命周期。

建议：把模式0结论改为显式待消歧，列明同版PDF469 UNPREDICTABLE与PDF473写忽略/PDF866返回的差异及产品未澄清范围；保持正常mode1、WO、无writer和不得依赖该非法序列的规则，不通过硬件试写判定。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:6620

### [P3] REAUDIT-026 四个LRC未完整说明无效pINTID及重复物理映射的UNPREDICTABLE结果

关联原问题：R52-128；寄存器：ich_lrc0, ich_lrc1, ich_lrc2, ich_lrc3。

四个LRC已补有效ID、物理状态和PPI作用域约束，但UNPREDICTABLE只明确附在pending/active时的物理状态前提上。原修正要求中的无效pINTID结果未明确写入，重复物理映射的同类结果也遗漏。不能将该结果缩限为物理状态前提失败或猜成写忽略/无事件；结构化RW和当前无writer未改变。

建议：在四个LRC根规则或共用且可追溯的条件布局说明中，分别写明HW=1时无效pINTID为UNPREDICTABLE、同一vCPU重复pINTID物理映射为UNPREDICTABLE；与pending/active物理active前提分开，不作用于HW=0的EOI低位布局，也不臆断全部SGI映射禁止。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:5796
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:5820
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:5844

### [P3] REAUDIT-027 100个MPU区域视图的多命中故障说明未保留推测访问例外

关联原问题：R52-165；寄存器：hprbar, hprbar0, hprbar1, hprbar10, hprbar11, hprbar12, hprbar13, hprbar14, hprbar15, hprbar16, hprbar17, hprbar18（共100项，完整范围见JSON）。

同bank多命中及包含式边界修复已覆盖全部100视图，但当前故障句未区分架构访问与推测事务。TRM同节明确推测越界/权限受阻时阻止总线事务而不报告Translation或Permission fault；仅据当前无条件表述会误解尚未完成执行的推测访问必产生可观察异常。寄存器编码/字段及无writer未受影响。

建议：将多命中Translation fault句限定为相应架构访问的故障规则，并补TRM9.1推测事务被MPU阻止总线访问且不报告这两种fault的例外；保留实际执行访问、同bank区分、两阶段合并和Permission fault独立规则，不扩张为软件可忽略真实重叠。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:1950
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:1972
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:1994

### [P3] REAUDIT-028 EDITR/EDSCR仍将R52已选定的在途指令重启结果保留为通用四选一

关联原问题：R52-152；寄存器：editr, edscr。

ITE就绪与完成的主要缺陷已修正，但对ITE0的已提交在途EDITR指令仍给出R52不选用的Non-debug完成/放弃等结果。原审计当前要求自身也保留通用四分支，不能据此循环证明产品描述正确；该范围已有明确产品证据，不是硬件未知。隐式Memory访问是否采用相同产品选项需另外限定，不能从EDITR表行泛化。

建议：用TRM C.6产品选定结果限定Normal模式已提交EDITR在途指令，包括ITE0场景；保留不能用ITE1证明完成、可能异常指令restart前观察和普通访问前提。通用架构四分支若保留，应明确仅是架构背景/尚未产品明确的适用范围，不再作为本产品EDITR可选结果。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9444
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9406

### [P3] REAUDIT-030 31个IGROUPR视图遗漏pending期间改组的有限时间与不丢失/重复规则

关联原问题：R52-161；寄存器：gicd.igroupr1, gicd.igroupr2, gicd.igroupr3, gicd.igroupr4, gicd.igroupr5, gicd.igroupr6, gicd.igroupr7, gicd.igroupr8, gicd.igroupr9, gicd.igroupr10, gicd.igroupr11, gicd.igroupr12（共31项，完整范围见JSON）。

组位/地址/权限与初始选择已补齐，但31个描述遗漏已公开的动态改组规则；总线写完成和RWP语义不能代替此保证，不能只据普通RW说明推导改组瞬间的投递。

建议：对全部30个GICD_IGROUPR及当前核GICR_IGROUPR0补pending期间旧值/新值选择、有限时间生效和不丢失/重复处理规则；可放入明确适用的共享组说明并可追溯，保持DS1、独立组使能和不新增writer。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-1.toml:96
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-1.toml:147
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-1.toml:198

### [P3] REAUDIT-031 10个GIC身份视图的8位容器遗漏已公开的体系结构子位域

关联原问题：R52-161；寄存器：gicd.pidr1, gicd.pidr2, gicd.pidr3, gicd.pidr4, gicd.cidr1, gicr.pidr1, gicr.pidr2, gicr.pidr3, gicr.pidr4, gicr.cidr1。

PIDR1/2/3/4和CIDR1两个组件各5个视图字节值/地址/RO正确，但缺ARCHREV、JEDEC/DES、PART高位、SIZE、CLASS/Preamble及版本字段的位义。Arm GIC子字段已在适用架构公开且产品值一致，不能作为未公开细节保持fields_missing=false的完整位域声明；PIDR身份标识架构方案，真实实现者/版本仍应查IIDR。

建议：按IHI0069H.b12.1.13为这10个视图拆分已公开子位域或在容器内明确完整位号/含义，并准确标记剩余缺口。ARCHREV=3对应GICv3、SIZE=4对应64KiB，CLASS=F；CMOD/RevAnd采用R52产品0而不推测其他实现。不新增ITS或export身份寄存器，不改变writer。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-4.toml:3260
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-4.toml:3281
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\r52-gic\gic-mmio-4.toml:3302

### [P3] REAUDIT-032 实际加载全部GIC MMIO视图但planned_classes概览漏计为0

关联原问题：R52-161；寄存器：gicd.ctlr, gicd.typer, gicd.iidr, gicd.igroupr1, gicd.igroupr2, gicd.igroupr3, gicd.igroupr4, gicd.igroupr5, gicd.igroupr6, gicd.igroupr7, gicd.igroupr8, gicd.igroupr9（共1519项，完整范围见JSON）。

registers_list和matrix明细完整、实际category统计正确，但planned_classes使用gicd/gicr而参考目录使用gic_distributor/gic_redistributor/gic_export，概览显示No matching catalogue entries。会误导条件目录覆盖评估；未发现因此增加writer、借错owner或遗漏加载。其他内置目录分组有独立命名，修正须兼容。

建议：让gic_mmio覆盖类别匹配实际参考目录的GIC分组，并兼容现有内置gicd/gicr命名；明确导出身份视图是否计入并给出一致口径。保留实际硬件支持unverified及明细，不用宣称硬件1519个地址。

- 当前定义：G:\Data\GitFiles\DebugTUI\src\registers\matrix.rs:349

### [P3] REAUDIT-033 6个视图的10个字段未准确限定已有证据不足范围

关联原问题：R52-063, R52-065；寄存器：dcisw, dccsw, dccisw, ctidevaff1, ctiauthstatus, pmdevtype。

原57项不足与来源记录仍保留在报告，但TOML使用者看到的这些字段把已确认布局/候选值与未充分证明的规范属性或产品选择混合为确定结论。确认的是说明限定不足，不据此宣称真实硬件值0/6或RES0错误。R52-063/065原要求的WriteThrough/维护顺序已修复，关联新增字段范围不改写原问题结论。

建议：给全部三条set/way维护指令高间隙/余SET位、CTI Aff3/reset、CTI认证复制条件及PMU MAJOR增加精确资料限制与适用版本说明。保守写零或候选值6/0可保留为建议/推断，但不能作为硬件已证实固定值。保留已知布局、RO/WO/RW、其他认证/identity字段和现有无writer边界。

来源判断：SRC-052, SRC-044, SRC-041, SRC-034。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:582
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:604
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:626

### [P3] REAUDIT-034 EDVIDSR遗漏无效sample时固定字段与同版伪代码的冲突

关联原问题：原212项无对应缺陷编号；新增来源/资料限制全局复核；寄存器：edvidsr。

同版DDI0568A.c无效sample的NS31/E3_29/bit28返回边界无法由正文和共享伪代码唯一确定。当前说明遗漏该冲突，读者可能把无效sample高位当可靠固定标志；未证明任何实际硬件分支，也没有把所有[28:8]都判UNKNOWN，27:8仍有一致零依据。属于原57项全局复核新增问题，原212项没有对应缺陷编号。

建议：明确区分有效sample及无效sample更新条件，保留PDF217固定字段与PDF301高4位UNKNOWN赋值双方及版本。将不足限于NS31/E3_29/bit28边界，保持E2/VMID有效性、27:8零、RO及无独立采样副作用；不猜实际分支或固定reset。

来源判断：SRC-159。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9514

### [P3] REAUDIT-035 四个TRCCIDR访问说明未保留Management/Trace分类冲突

关联原问题：原212项无对应缺陷编号；新增来源/资料限制全局复核；寄存器：trccidr0, trccidr1, trccidr2, trccidr3。

当前access_condition无条件采用Management：Coreoff及外部OSLock仍OK。同版IHI0064H.b总表345列Management而详细362..364列trace，表347/348给出不同状态响应。确认的是访问说明遗漏现存资料冲突，未证明真实产品是哪一种响应；普通RO身份值、DebugoffERROR和软件锁RO可读不受影响。原57项全局复核新增问题，不重复累加原212项。

建议：为全部4入口保留分类双方和版本/物理页码，将Coreoff/外部OSLock读资格列为精确不足范围，区分外部与MMIO路径；不把普通供电RO、bit图及固定产品字节也判未知，不新建writer。

来源判断：CORESIGHT-B2-R52-SRC-05。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15539
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15558
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:15578

### [P3] REAUDIT-036 PMCR两个DP字段未准确保留认证门控来源边界

关联原问题：R52-047；寄存器：pmcr, pmcr_mmio。

DP字段把产品non-invasive禁止句和R-profile HPMD条件合为一条操作规则；适用一词未说明CountEvents伪码没有认证门控及产品event/export语句的区别。当前未提供实际产品附加条件的证明；确认的是资料限制说明不足，不据此断言认证不会影响cycle。R52-047原同步/完成规则已修复，新增字段问题另列。

建议：对两DP字段补TRM433/436与DDI0568A.c144/222/235各自支持的范围及尚未明确的附加认证条件；保留已知bit5/RW/reset0、HPMD和独立使能/filter，不合并成已验证完整硬件门控。

来源判断：SRC-037。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:11157
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:11746

### [P3] REAUDIT-037 TDIR物理ICC_DIR陷入范围的资料限制未在当前入口说明

关联原问题：R52-123；寄存器：ich_hcr, icc_dir。

虚拟DIR必须陷入的部分已准确，但ICH_HCR.TDIR字段和物理ICC_DIR权限模板只列virtual。R52产品324虚拟限定与347无该限定、GIC512物理实现选择及397/474先检查后重定向的差异未保留。不能由TDS存在或TALL/TC已修复推断FMO=IMO=0物理TDIR行为；未证明任何真实芯片分支。

建议：在ICH_HCR.TDIR和物理ICC_DIR访问说明中增加精确物理分支不确定性、双方产品章节与架构实现选择，保持虚拟trap必选和其他HSTR/TC规则，不授权或禁止尚未证明的物理分支。

来源判断：SRC-102, SRC-115。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:5568
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:6142

### [P3] REAUDIT-038 外部PMCR遗漏EPMAD整字Error与功能伪代码之间的冲突

关联原问题：R52-047；寄存器：pmcr_mmio。

外部PMCR当前选择有详细功能与伪代码依据，尚不足以裁定EPMAD条件下usage表Error的范围。确定缺陷为限定说明遗漏，不能据此禁止整个外部PMU或宣称实际访问成功/失败。原同步及完成规则已修复，原R52-047具体结论不变。

建议：在此入口访问说明中保留PDF221与187/290双方及精确EPMAD/Hyp认证条件，注明整字结果未消歧，保留已知基础认证、锁、布局和P命令保护。

来源判断：SRC-036。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:11746

### [P3] REAUDIT-039 八个ETM TYPE字段遗漏数据地址比较器逐实例适用性限制

关联原问题：R52-037；寄存器：trcacatr0, trcacatr1, trcacatr2, trcacatr3, trcacatr4, trcacatr5, trcacatr6, trcacatr7。

字段位置及TYPE编码正确；不能由八套编码表证明八个实例均支持data，也不能凭资源数2任意排除六个索引。已有DVC0/2及even4/6 RES0说明处理的是data-value容量，不能代替data-address限制。原R52-037来源小节修正成立。

建议：对全部八个TYPE及相关CACVR/ViewData路径保留资源总量、详细表和SUPPDAC信息的对应缺口；不猜具体有效索引，不改已确认编码或instruction能力。

来源判断：SRC-032。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:14819
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:14847
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:14875

### [P3] REAUDIT-040 EDPRSR.SPD遗漏Core掉电返回值的同版资料冲突

关联原问题：R52-140；寄存器：edprsr。

R52未提供将该实现选择收窄为RAZ的具体依据。确认的是powerdown返回说明遗漏冲突；正常供电读清、Cold reset1和产品DoubleLock不清的已知规则仍成立，不扩展为整个EDPRSR未知。

建议：在SPD及整字读规则明确保留H6.6.2和H9双方，限定不足为powerdown返回选择，分别保留retention/no-clear、Cold1、software lock和产品DoubleLock条件。

来源判断：SRC-141。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\cortex-r52-system-registers.toml:9748

### [P3] REAUDIT-041 五个DWT功能字段遗漏capability guard的同版资料冲突

关联原问题：F429-022；寄存器：dwt.function0, dwt.function1, dwt.function2, dwt.function3。

功能/位号/四个实例及产品复位支持已确认，不能据未更正的guard反转支持极性或宣称F429字段实际不可写。确定缺陷为精确字段行为来源冲突未说明；不把所有DWT字段或产品地址、复位都判未知。原F429-022复位补齐具体结论不变。

建议：为FUNCTION0.CYCMATCH及四个EMITRANGE记录748 guard与719/725/730功能规则的冲突及适用版本；保留现有支持检查、模式限制和功能解释，未取得适用更正时不承诺精确冲突分支读回。

来源判断：SRC-153。

- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7428
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7452
- 当前定义：G:\Data\GitFiles\DebugTUI\reference\stm32f429-system-registers.toml:7480

## 独立离线验证与测试

已完成真实离线CLI18个用例：8个有效加载（两参考目录、M3/M4/M7/R52/R52+及相对继承覆盖）和10个预期拒绝（缺失GIC分片、重复父、父定义冲突、缺override、孤立override、超过4父、超过3层、超过4 MiB文件、超过16 MiB图、17节点）。拒绝用例的实际退出码1属于预期结果；所有case原始stdout/stderr以gzip无损保存。命令仅registers_list/registers_matrix/quit，未连接GDB/OpenOCD、未发送实际读写或调试命令。

新增register_reaudit_runtime七项均通过：全部1519 GIC视图/17752字段真实加载、全容量有效/非法/未知边界、28 ROM及GIC严格owner与地址计划、16 BAS分段512原始值向量、写载荷与读取解码分离/非法重叠、描述reader非法权限/位置/宽度/执行组合，以及M3/M4/M7/R52/R52+旧格式回归。合成地址与配置只证明软件规划，不证明实际硬件配置或原子访问。

- pwsh -NoProfile -File G:\Data\GitFiles\DebugTUI\scripts\test-and-clean.ps1 test --locked --offline --test register_reaudit：failed，退出码1，{'passed': 3, 'failed': 2, 'ignored': 0}；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\batch01-test.log
- pwsh -NoProfile -File G:\Data\GitFiles\DebugTUI\scripts\test-and-clean.ps1 test --locked --offline --test register_reaudit primary_r52_numproc_all_five_bit_patterns_with_unrelated_bit_noise：passed，退出码0，{'passed': 1, 'failed': 0, 'ignored': 0}；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\batch09-numproc-retry-test.log
- pwsh -NoProfile -File G:\Data\GitFiles\DebugTUI\scripts\test-and-clean.ps1 test --locked --offline --test register_reaudit primary_r52_numproc_all_five_bit_patterns_with_unrelated_bit_noise -- --exact：failed，退出码1，None；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\batch09-numproc-test.log
- pwsh -NoProfile -File G:\Data\GitFiles\DebugTUI\scripts\test-and-clean.ps1 test --locked --offline --test reference_register_fixes：passed，退出码0，{'passed': 18, 'failed': 0, 'ignored': 0}；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\existing-integration-test.log
- pwsh -NoProfile -File G:\Data\GitFiles\DebugTUI\scripts\test-and-clean.ps1 test --locked --offline --lib registers::：passed，退出码0，{'passed': 228, 'failed': 0, 'ignored': 1}；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\existing-library-test.log
- pwsh -NoProfile -File scripts/test-and-clean.ps1 test --locked --offline --test register_reaudit_runtime：passed，退出码0，{'passed': 7, 'failed': 0, 'ignored': 0}；日志 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\turn08-runtime-test-run.log

既有18项集成回归及228项库测试通过，1项硬件回放忽略且不计通过；适用于哈希保持的当前原输入。最初5项独立测试为3通过/2失败，失败断言及证据保持，用于REAUDIT-001/002；后续定向NUMPROC运行1通过/5过滤不改变原失败结论。新7项测试均通过也不能覆盖或消除那两条已确认问题。两种无读取路径的session分支实际先返回错误且不发送I/O，资格/显示缺陷与实际硬件读取能力分开。

## 输入、失败诊断与清理

冻结1563个输入文件及ST头文件补充清单持续核验；被审TOML/应用实现/旧测试/原审计与修复资料均未修改，已有工作树变化保留。独立脚本的缺省值、来源API、合成meta表和类别统计诊断单独保留；没有改旧期望来取得通过。上一轮服务端上下文压缩容量错误后从落盘检查点恢复，成功原输出只按哈希复用。失败合成输入/原始stderr也保留，历史子进程退出码未及时记录的局限明确记为未知，没有补造成功。

最终轮进行完整受影响字段/共用规则审查、逐要求验收和报告收尾，没有新增Cargo/Node/分发/打包构建。已按AGENTS.md执行cleanup-build.ps1 -Apply，退出0；target前后均0字节，本轮精确释放0字节。本Goal此前Cargo约5.33 GiB为日志四舍五入历史合计，37份已无损归档合成TOML原件精确清理21,107,989字节；修复阶段约10.38 GiB不计入本Goal本轮。未把历史释放量重复计作本轮。保留原始资料、全部页/源码/测试及失败证据、报告台账、基线、gzip和.fixture-store/旧清单、工具链、用户配置、会话附件及正式bin附件；未移动产物或清理其他失败/硬件轮次。

## 全部212项逐项结论

| 编号 | 复审结论 | 原问题 |
|---|---|---|
| F429-001 | 修复成立 | ACTLR.DISMCYCINT 将中断延迟影响写反 |
| F429-002 | 部分修复，仍有确定缺陷 | 继承结果中 70 个条目没有来源元数据 |
| F429-003 | 部分修复，仍有确定缺陷 | 六个专用核心寄存器只有通用占位描述 |
| F429-004 | 修复成立 | 280 个 NVIC 条目共用模板，缺少各类读写规则与位域 |
| F429-005 | 修复成立 | 系统空间目录遗漏只写 STIR 命令入口 |
| F429-006 | 修复成立 | MPU 三组 RBAR/RASR 别名访问窗口遗漏 |
| F429-007 | 修复成立 | 使用 r0p0 与 r0p1 的差异作为未裁定冲突，导致 ACTLR 字段遗漏 |
| F429-008 | 确定部分修复成立，合理资料限制保留 | AFSR 未说明写1清除的锁存状态规则 |
| F429-009 | 部分修复，仍有确定缺陷 | MPU 编码与编程约束描述不足 |
| F429-010 | 部分修复，仍有确定缺陷 | 特征寄存器多个多级能力字段未说明编码含义 |
| F429-011 | 部分修复，仍有确定缺陷 | 已公开的特殊写入状态和同步约束未完整描述 |
| F429-012 | 部分修复，仍有确定缺陷 | 348 个系统空间条目未说明程序访问的特权要求 |
| F429-013 | 修复成立 | 遗漏 DCRSR/DCRDR 核心调试传输入口 |
| F429-015 | 修复成立 | SysTick.TENMS 将校准时钟条件错误泛化为最大 HCLK |
| F429-016 | 确定部分修复成立，合理资料限制保留 | MVFR2 的产品实现依据不足，仍标为 high 且无存在条件 |
| F429-017 | 修复成立 | NVIC 第三bank的保留IRQ位置未被产品定义区分 |
| F429-018 | 确定部分修复成立，合理资料限制保留 | SP/LR/PC 的通用占位描述缺少核心角色和调试写入规则 |
| F429-019 | 部分修复，仍有确定缺陷 | MMFAR/BFAR 未说明 M4 共享物理存储和有效标志互斥 |
| F429-020 | 修复成立 | 新增28个DWT/FPB入口的51字段描述为空，另缺6个整字字段 |
| F429-021 | 修复成立 | FP_REMAP.RMPSPT继承整字RW，遗漏只读能力权限 |
| F429-022 | 修复成立 | 13个DWT/FPB入口遗漏产品明确的复位或部分复位规则 |
| F429-023 | 修复成立 | DWT地址比较缺32KB上限、掩码对齐和模式写入约束 |
| F429-024 | 修复成立 | FPB缺完整地址重建、remap限制及双使能规则 |
| F429-025 | 确定部分修复成立，合理资料限制保留 | DWT计数与PC采样缺运行使能、停机及读值语义 |
| F429-026 | 确定部分修复成立，合理资料限制保留 | DWT_FUNCTION缺模式编码、链接和保留取值规则 |
| F429-028 | 部分修复，仍有确定缺陷 | 遗漏ITM、最低TPIU接口和ETM-M4共93个来源项 |
| F429-029 | 确定部分修复成立，合理资料限制保留 | 遗漏四个与内核调试直接相关的DBGMCU集成寄存器 |
| F429-030 | 部分修复，仍有确定缺陷 | F429产品勘误对四类系统控制及浮点状态的限定未描述 |
| R52-002 | 确定部分修复成立，合理资料限制保留 | 51个 EL1 MPU 访问条目遗漏 HSTR.T6 陷入条件 |
| R52-003 | 修复成立 | MPU选择器缺少选择写入到间接数据访问的同步规则 |
| R52-004 | 修复成立 | 定时器访问控制及虚拟偏移缺少依赖使用的同步说明 |
| R52-005 | 修复成立 | FPSCR访问条件遗漏FPEXC.EN及其异常优先顺序 |
| R52-007 | 修复成立 | Debug/CTI CLAIM共享状态缺少访问路径可见性与同步规则 |
| R52-008 | 确定部分修复成立，合理资料限制保留 | 整个处理器簇共享ROM表被错误标记为每核scope |
| R52-013 | 确定部分修复成立，合理资料限制保留 | TRCITCTRL遗漏进入集成模式的enabled/idle前置限制 |
| R52-014 | 修复成立 | TRCSTATR.PMSTABLE遗漏状态有效条件 |
| R52-015 | 修复成立 | TRCPDSR遗漏锁状态有效性及sticky复位置位来源 |
| R52-016 | 修复成立 | ETM七十入口的使能说明遗漏OS Lock联合条件 |
| R52-018 | 修复成立 | ETM十个事件选择入口把保留pair0列为可用范围 |
| R52-019 | 修复成立 | single-shot初始化说明仅覆盖RST0，遗漏STATUS普遍初始化要求 |
| R52-020 | 修复成立 | ETM计数控制缺少reload优先级及链式self-reload条件 |
| R52-021 | 部分修复，仍有确定缺陷 | sequencer描述遗漏同周期优先级和多级状态迁移 |
| R52-023 | 修复成立 | TRCCONFIGR数据跟踪使能遗漏INSTP0非零依赖 |
| R52-024 | 修复成立 | STALL控制遗漏侵入式调试全禁用时的忽略规则 |
| R52-025 | 修复成立 | 非零LEVEL可能抑制时间戳和周期计数输出的规则缺失 |
| R52-026 | 修复成立 | 分支广播范围遗漏必须为指令地址比较器的限制 |
| R52-029 | 修复成立 | SSSTATUS遗漏未用start-stop时置1及推测状态读回限制 |
| R52-030 | 修复成立 | start-stop选择缺少同时命中的冲突规则 |
| R52-031 | 修复成立 | ViewInst过滤和启停未限定指令地址比较器类型 |
| R52-032 | 修复成立 | ViewData include/exclude缺少比较器类型和data-value敏感性限制 |
| R52-033 | 修复成立 | VDARCCTLR把已实现比较器的初始化条件写成正在使用 |
| R52-034 | 部分修复，仍有确定缺陷 | ViewData缺少空include默认值和两地址域组合规则 |
| R52-038 | 修复成立 | Context ID掩码遗漏初始化与被屏蔽byte必须清零 |
| R52-039 | 部分修复，仍有确定缺陷 | Context ID和VMID比较遗漏PE更新至同步间的旧新值窗口 |
| R52-041 | 确定部分修复成立，合理资料限制保留 | 数据值比较遗漏single与range不同的访问大小约束 |
| R52-044 | 修复成立 | PMU软件锁入口未展开PADDRDBG31高低的写限制 |
| R52-045 | 修复成立 | 外部PMSWINC错误承诺PE指令访问的精确递增 |
| R52-047 | 修复成立 | PMCR两访问视图缺少依赖生效的同步与完成规则 |
| R52-048 | 修复成立 | 外部PMSWINC套用了不适用的Hyp位图屏蔽保证 |
| R52-049 | 修复成立 | PMU越界访问沿用通用不可预测选项，遗漏R52已明确的实现行为 |
| R52-050 | 确定部分修复成立，合理资料限制保留 | CTI软件触发命令缺少使能及multicycle能力条件 |
| R52-051 | 修复成立 | CTIINTACK只列debug/restart例子，遗漏通用确认顺序 |
| R52-052 | 修复成立 | GLBEN禁用说明遗漏既有multicycle输出channel终止 |
| R52-053 | 确定部分修复成立，合理资料限制保留 | CTI通道状态缺少无multicycle时必须视UNKNOWN的规则 |
| R52-054 | 确定部分修复成立，合理资料限制保留 | CTIDEVARCH忽略INOUT已证明的CTIv2身份，将三个固定字段继续列为未确认 |
| R52-055 | 修复成立 | ACTLR2访问及HCR.TAC范围遗漏ACTLR2陷入规则 |
| R52-056 | 部分修复，仍有确定缺陷 | EL2专用入口遗漏EL1尝试访问的HSTR陷入分支（T0/T1/T2/T6/T10/T12/T15） |
| R52-057 | 确定部分修复成立，合理资料限制保留 | HACTLR.TESTR1将仅供Arm测试的WO目标无依据地描述为全字RAZ/WI |
| R52-058 | 确定部分修复成立，合理资料限制保留 | HCPTR功能trap说明遗漏有条件的FPEXC.EN先行检查 |
| R52-059 | 修复成立 | HCR.TID3受控ID清单遗漏已实现ID_MMFR4 |
| R52-060 | 修复成立 | HCR.TWE/TWI遗漏EL0的SCTLR优先级及条件执行限制 |
| R52-061 | 修复成立 | HCR.DC/VM将强制有效VM规则弱化并遗漏缓存失效升级 |
| R52-063 | 修复成立 | 五个clean操作入口未区分R52的Write-Through实现与通用脏line定义 |
| R52-064 | 修复成立 | BPIMVA.VA遗漏必须使用待失效分支指令自身地址 |
| R52-065 | 修复成立 | 十三个缓存维护入口的共享顺序模板遗漏屏障类型和预测器维护条件 |
| R52-066 | 修复成立 | FI/S2NIE遗漏Normal多字访问的恢复语义及FI生效条件 |
| R52-067 | 确定部分修复成立，合理资料限制保留 | 两个ITD字段遗漏活动IT块内自行置位限制，HS另漏条件失败仍UNDEFINED |
| R52-069 | 修复成立 | Cache路划分控制遗漏复位后首次启用前的写入生命周期限制 |
| R52-070 | 修复成立 | 四个Cache控制入口遗漏HSTR的写访问或64位访问陷入分支 |
| R52-071 | 修复成立 | Cache选择器与几何读取之间遗漏必要的上下文同步 |
| R52-072 | 修复成立 | 不存在的Cache选择只留笼统提示，未说明读值与异常边界 |
| R52-073 | 修复成立 | IMP_CDBGDCI遗漏HACTLR.CDBGDCI与HSTR.T15访问控制 |
| R52-074 | 确定部分修复成立，合理资料限制保留 | 四个fault入口遗漏HSTR分支且Hyp辅助状态的TVM来源冲突未消歧 |
| R52-075 | 修复成立 | Hyp三故障地址记录遗漏低异常级别执行即UNKNOWN的有效期 |
| R52-076 | 修复成立 | DFSR.ExT遗漏非外部Abort时返回0 |
| R52-077 | 修复成立 | HSR异常入口更新说明遗漏IRQ/FIQ不写syndrome的例外 |
| R52-080 | 修复成立 | ISR.A/I/F遗漏当前EL和HCR控制的物理虚拟来源选择 |
| R52-081 | 确定部分修复成立，合理资料限制保留 | 内存接口九入口遗漏HSTR访问陷入，Flash另缺HACTLR写陷入结果 |
| R52-082 | 修复成立 | Flash与LLPP使能描述遗漏已实现但禁用时的同步外部中止 |
| R52-083 | 确定部分修复成立，合理资料限制保留 | TCM基址复位资料冲突尚未消歧却在概要中固定选择0 |
| R52-084 | 修复成立 | RAMPROTEN缺少TCM开启ECC后的读取初始化前提 |
| R52-086 | 修复成立 | 区域映射说明遗漏TCM多命中与外部端口窗口重叠限制 |
| R52-087 | 修复成立 | 错误记录与TCM syndrome十入口遗漏HSTR.T15陷入条件 |
| R52-088 | 修复成立 | 八个错误记录缺少空槽分配、fatal覆盖及记录丢失规则 |
| R52-091 | 确定部分修复成立，合理资料限制保留 | 六个实现配置、监控与测试入口遗漏通用HSTR.T15陷入控制 |
| R52-093 | 修复成立 | PAR两个访问视图缺少地址转换结果的明确上下文同步规则 |
| R52-095 | 修复成立 | MAIR/HMAIR属性描述遗漏R52数据缓存的WT和Shareable限制 |
| R52-096 | 修复成立 | FCSEIDR兼容入口缺少明确的EL1 HSTR.T13陷入条件 |
| R52-099 | 修复成立 | MPUIR读取权限遗漏HSTR.T0先行陷入条件 |
| R52-100 | 修复成立 | HPRENR两字段否认未实现区域位的写忽略规则 |
| R52-102 | 修复成立 | HDCR.TDA/TDE遗漏Debug state下DTR访问不受相应陷入控制的例外 |
| R52-104 | 修复成立 | ICC_PMR/ICV_PMR遗漏GIC专用HSTR.T12陷入分支 |
| R52-106 | 修复成立 | 中断组使能遗漏观察顺序及物理禁用时的待中断释放 |
| R52-111 | 确定部分修复成立，合理资料限制保留 | IAR确认读取遗漏条件自同步及物理确认可观察规则 |
| R52-113 | 修复成立 | 六个EOIR/DIR入口遗漏去激活命令的条件同步及可观察对象 |
| R52-117 | 修复成立 | 四个虚拟APR视图遗漏相同优先级槽的跨Group互斥要求 |
| R52-118 | 修复成立 | ICH_AP0R0和ICH_AP1R0遗漏强制保存恢复写入顺序 |
| R52-121 | 修复成立 | 三个SGI生成入口遗漏命令的条件可见性同步规则 |
| R52-123 | 修复成立 | ICH_HCR三个陷入位未明确同样覆盖未重定向的EL1物理ICC访问 |
| R52-124 | 修复成立 | ICH_VMCR组使能别名遗漏条件性的Redistributor观察同步规则 |
| R52-127 | 确定部分修复成立，合理资料限制保留 | 四个LR低字遗漏特殊vINTID和有效条目唯一性限制 |
| R52-128 | 部分修复，仍有确定缺陷 | 四个LRC遗漏物理映射的有效ID、物理active前提和PPI作用域 |
| R52-129 | 修复成立 | LRC.State未限定pending+active仅用于软件来源中断 |
| R52-130 | 修复成立 | LR上下文更新遗漏EOImode0活动优先级一致性与唯一性约束 |
| R52-131 | 修复成立 | 五个EL1 SPSR银行将Hyp已实现误写为合法返回模式 |
| R52-132 | 修复成立 | 六个保存状态入口误用普通MSR字节掩码规则 |
| R52-133 | 修复成立 | CPSR模式写入遗漏从Hyp切换到其他模式的禁止条件 |
| R52-136 | 修复成立 | Context链接比较器遗漏独立触发限制及被忽略控制字段 |
| R52-139 | 修复成立 | 外部OS Lock认证拒绝错误套用通用Debug实现定义规则 |
| R52-140 | 确定部分修复成立，合理资料限制保留 | EDPRSR粘滞字段读清除错误增加reset条件并遗漏OS Lock限制 |
| R52-141 | 修复成立 | OS Lock保存与Double Lock掉电步骤遗漏生效前的同步要求 |
| R52-143 | 修复成立 | EDITR提交说明遗漏MA模式与累计错误门控 |
| R52-144 | 部分修复，仍有确定缺陷 | 两个外部DTR入口缺具体Memory模式事务和ERR流控规则 |
| R52-145 | 修复成立 | 内部DTR数据访问缺Non-debug显式同步及状态先检查规则 |
| R52-146 | 修复成立 | EDITR高半字说明遗漏16位T32在Debug state的UNPREDICTABLE限制 |
| R52-147 | 确定部分修复成立，合理资料限制保留 | EDECCR及OS保存视图未展开coarse catch的异常返回触发 |
| R52-148 | 修复成立 | Halting step使能/状态描述缺R52 Hyp异常规则和状态迁移 |
| R52-149 | 修复成立 | Reset/OSUnlock pending描述缺退出清除和解锁事件生效条件 |
| R52-150 | 修复成立 | DBGVCR地址匹配描述遗漏普通分支、Abort递归和T32邻半字限制 |
| R52-151 | 修复成立 | Exception/执行控制缺访问完成与控制效果同步的区别 |
| R52-152 | 部分修复，仍有确定缺陷 | EDSCR与EDITR把ITE就绪当作已提交指令执行完成 |
| R52-153 | 修复成立 | DSCR错误标志遗漏Memory模式未就绪及精确触发谓词 |
| R52-154 | 修复成立 | ERR及EDRCR.CSE遗漏执行抑制和清错前ITE条件 |
| R52-155 | 修复成立 | DBGDSCRint实时轮询描述缺跨agent通知后的同步边界 |
| R52-156 | 修复成立 | DSCR控制缺新控制生效的上下文同步要求 |
| R52-158 | 修复成立 | EDSCR.HDD缺Hyp调试禁用时的指令及异常结果 |
| R52-159 | 修复成立 | DSCR满空位缺Memory就绪组合与异常后UNKNOWN边界 |
| R52-160 | 修复成立 | DBGDSCRext错误保存恢复缺RXO/TXU与ERR一致性约束 |
| R52-161 | 部分修复，仍有确定缺陷 | 遗漏处理器集成GIC Distributor及Redistributor的全部MMIO接口 |
| R52-162 | 修复成立 | 37个GIC CPU接口可写入口遗漏Redistributor休眠状态写入限制 |
| F429-027 | 部分修复，仍有确定缺陷 | 遗漏SCS/FPB/DWT组件身份及ROM发现接口共53个来源项 |
| R52-001 | 修复成立 | HPRSELR 用途描述误写 EL1 银行寄存器名 |
| R52-006 | 部分修复，仍有确定缺陷 | FPSID未说明合法VMSR写入为忽略 |
| R52-009 | 确定部分修复成立，合理资料限制保留 | 处理器CLAIM描述无依据地把CTI固定为四个tag |
| R52-010 | 修复成立 | TRCLSR复位描述混入UNKNOWN模板，与路径相关已知值矛盾 |
| R52-011 | 修复成立 | PMPIDR0/1的part枚举标签误指CTI组件 |
| R52-012 | 修复成立 | TRCIDR3.NUMPROC分段拼接顺序反向 |
| R52-017 | 修复成立 | TRCDEVAFF0把配置相关物理身份写成UNKNOWN复位模板 |
| R52-022 | 修复成立 | 奇数TRCRSCTLR的RES0 PAIRINV仍枚举为可取反功能 |
| R52-027 | 修复成立 | 周期阈值存储范围未清楚区分合法编程范围及违规后果 |
| R52-028 | 确定部分修复成立，合理资料限制保留 | 时间戳说明遗漏系统实际提供timestamp source的前提 |
| R52-035 | 部分修复，仍有确定缺陷 | PCREL和SPREL过滤遗漏base加立即数偏移的限定 |
| R52-036 | 确定部分修复成立，合理资料限制保留 | VDSACCTLR文字限定八位但EXCLUDE结构仍提取九位 |
| R52-037 | 修复成立 | 八个ADDRESS字段的架构小节号误指外部输出 |
| R52-040 | 修复成立 | CONTEXT全字段RES0被描述成可选择不存在对象 |
| R52-042 | 修复成立 | 地址和数据比较遗漏Debug state不产生匹配的条件 |
| R52-043 | 确定部分修复成立，合理资料限制保留 | PMCFGR.N把计数器实现值误引到访问偏移表 |
| R52-046 | 修复成立 | PMSWINC命令位同时描述无保持状态和未知复位状态 |
| R52-062 | 修复成立 | ICIALLU/BPIALL及ID_MMFR3.MaintBcst缺R52维护仅本核的产品限定 |
| R52-068 | 修复成立 | SCTLR.I的MPU关闭默认属性引用遗漏R52产品背景映射限定 |
| R52-078 | 部分修复，仍有确定缺陷 | HSR.ISS错误合并0x34与0x35的R52产品来源结论 |
| R52-079 | 修复成立 | HVBAR缺少写后上下文同步及隐含同步事件说明 |
| R52-085 | 确定部分修复成立，合理资料限制保留 | ABORT三字段未明确区分超时中止使能与超时发生状态 |
| R52-089 | 修复成立 | DCERR和FLASHERR四入口遗漏产品明确的零复位值 |
| R52-090 | 修复成立 | FLASHERR.LATE缺少可纠正、重新执行及不产生Abort的含义 |
| R52-092 | 确定部分修复成立，合理资料限制保留 | TESTR0四个IRQ/FIQ字段的高有效解释缺直接适用证据 |
| R52-094 | 修复成立 | PAR复位描述遗漏资料明确给出的UNKNOWN属性 |
| R52-097 | 修复成立 | VPIDR/VMPIDR未说明虚拟ID跨视图使用的上下文同步边界 |
| R52-098 | 修复成立 | ID_MMFR3两个Cache维护特征字段未展开编码1所支持的具体操作 |
| R52-101 | 修复成立 | HDCR.EPMAD遗漏外部PMCFGR数量可见性及PMCR.P复位保护 |
| R52-103 | 修复成立 | ICC_HSRE根描述将内容只读误写成不存在允许的软件写访问 |
| R52-105 | 修复成立 | 物理和虚拟PMR遗漏写入自同步的明确保证 |
| R52-107 | 确定部分修复成立，合理资料限制保留 | 只读虚拟RPR/IAR/HPPIR的根描述错误包含MCR写访问 |
| R52-108 | 部分修复，仍有确定缺陷 | 虚拟组使能位缺少全局EN及组状态维护中断依赖 |
| R52-109 | 修复成立 | BPR1位域给出的最小值钳制规则PDF页码不符 |
| R52-110 | 修复成立 | HPPIR的INTID说明遗漏与IAR不同的门限及跨组选择规则 |
| R52-112 | 确定部分修复成立，合理资料限制保留 | 三个虚拟只写EOIR/DIR根描述仍包含MRC和可读模板 |
| R52-114 | 修复成立 | 物理DIR.INTID遗漏GICv3边界行为和去激活PE范围 |
| R52-115 | 修复成立 | 虚拟EOIR1对TRM笔误的说明引用了错误表号 |
| R52-116 | 部分修复，仍有确定缺陷 | 虚拟EOIR/DIR的INTID说明遗漏LR状态、缺失LR计数和维护条件 |
| R52-119 | 修复成立 | 两个ICV APR的恢复规则引用了物理ICC章节 |
| R52-120 | 修复成立 | 128个虚拟APR位域的0/1说明缺少priority-drop生命周期限定 |
| R52-122 | 确定部分修复成立，合理资料限制保留 | 三个TargetList字段遗漏无效目标位的处理语义 |
| R52-125 | 修复成立 | ICH_MISR.NP未明确pending仅指LR.State=01 |
| R52-126 | 修复成立 | ICH_ELRSR.Status为0的含义概括过强，与安全复用谓词存在例外 |
| R52-134 | 修复成立 | CPSR复位说明遗漏已知A/I/F和Hyp模式值 |
| R52-135 | 修复成立 | DLR字段把复位后进入Halting Debug的有效PC一并排除 |
| R52-137 | 修复成立 | 观察点LBN遗漏非链接模式的UNKNOWN读回规则 |
| R52-138 | 修复成立 | 外部Debug访问模板误用于只写锁入口及具有核内映射的身份字 |
| R52-142 | 修复成立 | DBGDCCINT未说明中断控制写后的同步边界 |
| R52-157 | 修复成立 | EDSCR遗漏MA与STATUS的部分已知Cold复位信息 |
| R52-163 | 确定部分修复成立，合理资料限制保留 | 53个GIC CPU接口入口未区分处理器Warm reset域与GIC reset域 |
| F429-031 | 确定部分修复成立，合理资料限制保留 | 缺少已由F429原始手册确认的TPIU格式化控制和状态寄存器 |
| F429-032 | 修复成立 | FPSCR的FZ、DN和V描述遗漏或混淆具体操作范围 |
| R52-164 | 修复成立 | WSTRNOL1ACTL遗漏写流的生效边界和退出条件 |
| R52-165 | 部分修复，仍有确定缺陷 | 全部MPU区域视图遗漏重叠命中的Translation fault规则 |
| R52-166 | 修复成立 | MAIR和HMAIR概要混用两个区域银行的属性索引来源 |
| R52-167 | 修复成立 | ETACDIS遗漏异常目标缓存的TCM、指令和异常级边界 |
| R52-168 | 修复成立 | 全局MPU使能遗漏先编程并使能至少一个区域的前提 |
| R52-169 | 修复成立 | 属性字节遗漏R52内部Device处理与外部总线的区别 |
| R52-170 | 确定部分修复成立，合理资料限制保留 | 全部断点控制视图遗漏R52已选择的保留及匹配行为 |
| R52-171 | 确定部分修复成立，合理资料限制保留 | 全部观察点视图遗漏R52 MASK、字节选择及非法链接结果 |
| R52-172 | 修复成立 | EDPRSR遗漏Double Lock时不清除sticky的R52产品选择 |
| R52-173 | 修复成立 | ViewInst入口遗漏产品事件精度及双发射指令跟踪边界 |
| R52-174 | 确定部分修复成立，合理资料限制保留 | 全部Hyp MPU基址/上界视图遗漏EL1尝试访问的HSTR.T6陷入分支 |
| R52-175 | 修复成立 | 全部MPU基址SH描述遗漏Device及双Non-cacheable内存的忽略规则 |
| R52-176 | 修复成立 | 旧CP15 DMB/DSB描述遗漏R-profile的VMID及异常级排序范围 |
| R52-177 | 确定部分修复成立，合理资料限制保留 | EDPRCR.CWRR遗漏Hyp invasive认证禁止时忽略写入的R-profile限制 |
| R52-178 | 修复成立 | DLR/DSPSR遗漏Debug state异常及DCPS/DRPS导致保存状态失效 |
| R52-179 | 修复成立 | HCR.VA遗漏虚拟SError异常接收后自动清零 |
| R52-180 | 修复成立 | DBGDSCRint读取说明遗漏R15/APSR_nzcv特殊目标的标志更新和Debug限制 |
| R52-181 | 确定部分修复成立，合理资料限制保留 | 八个保存状态入口的T位遗漏IL=1时零或保存值的恢复限制 |

逐项完整范围、原缺陷/修复声明、修复前/当前/最终定义定位、独立原始资料、方法、实际结果、剩余限制和修正建议见同名JSON；独立脚本、页证据、日志及归档位于 G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429。

## 最终完成证据

G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\turn12-complete-affected-semantic-scope-review.json
G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\turn12-goal-requirements-audit.json
G:\Data\GitFiles\DebugTUI\artifacts\system-registers-reaudit-r52-f429\turn12-report-consistency.json
