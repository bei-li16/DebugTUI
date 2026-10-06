# M Debug/DWT/FPB/FPU 延后环境用例

全部实板用例当前为 **SKIPPED**。本 Goal 不执行上板；下面的驱动和独立期望值用于环境允许时执行。仅软件 EXE/MI 夹具运行不能升级硬件 verified 标记。

默认执行，零连接、零目标 I/O：

```powershell
node scripts/test-m-profile-modules-hardware.cjs
```

显式执行（替换实际路径与核名）：

```powershell
node scripts/test-m-profile-modules-hardware.cjs --run --binary C:\path\debugtui.exe --project C:\path\debug.toml --core core0 --case C:\path\m-module-board.json
```

从 `tests/fixtures/m-profile-modules-board.example.json` 复制模板，填写真实型号/revision、AP、target、endpoint、探针和工具版本、停止帧函数及独立数据来源，改 `software_example=false`。GDB/Tcl 项目路由必须属于所选核。期望值从可信固件、独立调试器或先前独立采样取得，不能从本次 DebugTUI 输出回填；保留采集步骤和工具版本。`manual_dhcsr=true` 是此用例对一次副作用读取的明确许可，不读取任意副作用项。DHCSR mask/value 只验证稳定的已知位，如 S_HALT；不要预设异步粘滞位。

| Case | 当前状态 | 步骤与独立预期 |
| --- | --- | --- |
| M-MOD-H01-IDENTITY | SKIPPED | 在 M3/M4/M7 物理停止帧确认目录及 CPUID，与独立 baseline 一致；来源 owner/context/endpoint 匹配，错误核不得授予可选模块 |
| M-MOD-H02-DEBUG | SKIPPED | 确认既有 DEMCR.TRCENA 与 FP_CTRL 原始值及容量；关闭 TRCENA 时 DWT 为 NeedEnable、无数据读取；另备已启用固件/状态验证 DWT NUMCOMP。驱动不更改使能、比较器或计数器 |
| M-MOD-H03-DHCSR | SKIPPED | 自动请求为 NotRead、无 access；显式单次读取返回带来源的 DHCSR；手动读影响粘滞位，完成后继续确认 OpenOCD/GDB 正常管理核状态 |
| M-MOD-H04-FPU | SKIPPED | M4/M7 有效 MVFR0、16 D/32 S 容量与独立 CPACR/FPCCR/FPCAR/FPDSCR、D0/S0/S1/FPSCR 一致；D/S 同批共用来源。准备 CPACR 禁止、仅特权和完全访问的独立固件状态，分别记录后端实际成功或拒绝；无 FPU 项目不尝试数据读取，M3 此阶段不适用 |
| M-MOD-H05-UNCHANGED | SKIPPED | 重读安全配置与前值一致、项目摘要不变，无 DebugTUI 写入或运行控制命令；运行后检查 FPU、DEMCR 与 FPB 没有持久改变。DHCSR 粘滞位不计为需恢复的普通配置 |

多核扩展：在 M7＋M4 或同型多核分别使用独立 case 和核名，记录两核 CPUID、容量、CPACR 和 GDB 原始数据。先单核运行，再切核；不得仅凭同一 PPB 地址复用 peer 值。该模块用例不替代 B10 的完整目录/错误/缓存生命周期验收。

错误、权限、缺名、无效 MVFR0、证据过期/撤销及未使能状态的自动零 I/O由本机生产 session 集成测试覆盖；硬件上异常状态通过已有固件或独立工具准备，再连接本驱动，不通过驱动写入使能或权限。失败即停止后续阶段并记录失败/跳过，不重试或更改目标配置以换取通过。

恢复方式：驱动关闭自己的会话，不复位或改配置；手动 DHCSR 读已清除的粘滞位无法写回，保留读前独立证据。若外部工具为准备前置条件改变了状态，由该工具按板级说明恢复并记录。报告保存各 phase、版本、输入和源码/项目/二进制摘要。实际 AP 映射仍需板级独立核对，软件 endpoint 证据不等同物理 AP 证明。
