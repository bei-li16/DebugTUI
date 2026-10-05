# REG-110 条件依据环境 case

初始状态：以下八项均 **SKIPPED**。用户要求本轮不执行上板测试；现有测试只提供软件证据。执行时保存开发提交、EXE/目录/profile SHA256、实际 DebugTUI/GDB/OpenOCD 版本、物理核/模式/停止位置、完整 JSON/MI/TCL 日志和 Status 截图，逐项记录差异。

使用审核的工程与固件，逐核暂停于物理 frame 0。先获取 `registers_list` 当前 context，显式 `registers_probe`，然后按需 `registers_read`；具体 RPC 参数以当前帮助和既有能力 case 为准。切核、换帧、运行或重连后重新获取 context。System Regs 中选条目按 `t`，方向键/滚轮逐页查看条件依据，Esc 关闭。不要为了测试隐式设置 CPU 模式、FPU 权限或 PMU enable/reset；受控 Guest 环境由已审核固件建立。

| ID | 操作与独立核对 | 通过条件 | 状态 |
| --- | --- | --- | --- |
| ELIG-H01 | 实际 R52 Hyp 下显式 Probe，核对 MIDR、CPSR、PMCR.N=4 与 ICC_CTLR.PRIbits=4；配置故意使用不同声明值，再读安全条目 | 当前观察优先，原声明独立保留；CPU/目录/命令/原始字段/时间与日志一致；PMCR.E=0 也不否定数量，不启用计数器 | SKIPPED |
| ELIG-H02 | 固件自然处于 Guest/SVC 且 HPMN 限制为 0/1 时读取能力；在安全环境注入 Hyp 的未适配 N 或物理 ICC 值的软件服务 | EL0/EL1 N 不成为物理 count；原字段可查，无声明时 optional Unknown 不自动读；不匹配 R52 物理五位保持 Unknown，虚拟 ICH 不替代 | SKIPPED |
| ELIG-H03 | 实际 FPU 已实现但未使能/受限，以及身份或能力不可读场景；分别查看和手动请求合法条目 | 有已验证实现依据时 Yes 与访问拒绝分开；缺少能力时 Unknown，disabled/denied/unsupported/error 均不虚报未实现；不改 FPU 控制 | SKIPPED |
| ELIG-H04 | 私有目录使用 optional 父链 alias、min/max 边界、WO 和安全模拟副作用；逐项自动与明确手动请求，保存独立服务日志 | 子项继承全父链；Unknown 自动不读、No 手动不读、WO 从不读；安全 manual 副作用仅一次；条件5/6/7测试不冒充R52物理能力 | SKIPPED |
| ELIG-H05 | 两核各有不同能力/声明；Scope All，切核、换帧、继续/停止、重连，并提交旧 context 请求 | 仅当前 worker Probe/read；peer 无新增操作；旧 session/stop/core/frame 不作为当前依据；历史样本保留自己的 context | SKIPPED |
| ELIG-H06 | 有成功旧值后连续注入权限错误/后端 unsupported/断连；在独立共享域测试服务中使 peer 活动导致新值被拒绝，然后再失败刷新 | 最新条件/失败原因和原值依据分开；中间 Snapshot、Response、后续 frame/status 只能保留最后认可的 raw/route/basis；被拒绝字节及证据不复活 | SKIPPED |
| ELIG-H07 | 导入缺少 eligibility 的旧格式快照；比较当前样本、连续失败样本的 JSON；保持旧平面寄存器客户端 | 原值依据明确 Unknown，不拼凑新依据；新字段往返完整；旧 JSON 和平面列表可正常读取 | SKIPPED |
| ELIG-H08 | Windows Terminal/PowerShell 与 VS Code 终端，45×12、80×24、120×36，中文目录/名称及长来源，逐页查看 current/latest/retained | 所有条件/范围/目录/原始源可滚动，最新与原值 stop/core 清楚；查看无新增 MI/TCL，保存前后文件和样本一致；仅本项不能认定 REG-208 全部完成 | SKIPPED |

ELIG-H02 的实际模式及 HPMN 必须由固件独立记录，不能靠 debugger 改模式制造证据。数量变化的软件服务与真实板卡结果分别记录。任何恢复结果未知都沿用 FAULT/reconnect 流程，不自动重试；失败或缺少环境保持 SKIPPED，不填写 PASS。
