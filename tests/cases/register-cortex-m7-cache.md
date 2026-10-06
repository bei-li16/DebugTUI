# M7 cache/TCM 延后环境用例

本 Goal 不执行上板。下面四阶段当前为 **SKIPPED**，不能据软件夹具宣称硬件通过或升级 verified。默认驱动不启动 debugger、不连接目标、不读取寄存器。

```powershell
$env:DEBUGTUI_TEST_ARTIFACT_ROOT = 'C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\evidence'
node G:\Data\GitFiles\DebugTUI\scripts\test-m7-cache-hardware.cjs
```

日后具备环境并明确安排上板时，将 `G:\Data\GitFiles\DebugTUI\tests\fixtures\m7-cache-board.example.json` 复制为实板 JSON；不能照抄 software_example 的 cache/TCM 参数。填入独立审阅的 CPUID/revision、CLIDR/CTR、原 CSSELR、已实现 I/D cache 的完整 CCSIDR 和配置原值，并声明工具版本、探针、AP、OpenOCD target、停止位置及来源。将 software_example 改为 false。每核项目配置必须提供独立的 CorePrivate target/AP 路由；测试前自行暂停在 physical frame 0，不在驱动中 halt/resume/reset。

```powershell
node G:\Data\GitFiles\DebugTUI\scripts\test-m7-cache-hardware.cjs --run --binary C:\Tools\debugtui.exe --project C:\Cases\m7-debug.toml --core core0 --case C:\Cases\m7-cache-expected.json
```

| Case | 操作及独立预期 | 恢复与报告 | 当前 |
| --- | --- | --- | --- |
| M7-CACHE-H01-IDENTITY | 核对 STOPPED/frame0/停止函数/目录、实际 CPUID/CLIDR/CTR 和 Probe facts；分别准备无 cache、仅 I、仅 D、I+D 的适用配置 | 不写配置；前置失败跳过后续 | SKIPPED |
| M7-CACHE-H02-CONFIG | 读取声明的 CCR/CACR/ITCMCR/DTCMCR/AHBPCR/AHBSCR，与独立配置基线逐项比较；有 cache 时保存 CSSELR 原值 | 保留原始响应；不自动使能或执行维护 | SKIPPED |
| M7-CACHE-H03-BANKS | `registers_cache read:true`，验证每个已实现 bank 的 selector/type/raw、core owner/context/target 和完成请求区间；无 cache 不出现 selector 样本 | 原 CSSELR 必须恢复且回读一致；完整 evidence JSON | SKIPPED |
| M7-CACHE-H04-RESTORE | 再读配置/CSSELR，与 H02 完全相等；客户项目 hash 不变；MI 无运行或数据写命令 | 关闭本测试 session，不复位硬件；JSON report/MI/Tcl 日志 | SKIPPED |

错误基线必须失败，不能只检查“请求成功”。例如独立 CCSIDR 的期望值故意改为其他容量编码，应在 H03 判失败；H04 不运行，报告明确记录该区别。用于验证驱动的 `--software-fixture` 明确标记 board_tests_executed=false，真实硬件不使用该参数。

双核复验：在实际多核板上为每个 M7 配置独立 AP/target/端口和独立 case，依次运行上述命令；在 TUI 切核后分别 Probe/Read，核对原 selector、缓存 owner 和各核容量，不由相同 PPB 地址推断同一对象。M7＋M4 时，M4 使用已准备的 M MPU/Debug 用例；M4 的 `:cache` 应拒绝且零 cache I/O。本项属于 B10 的完整多核验收，不能用本批双 M7 软件测试替代实板。

恢复故障/断线在软件 fixture 覆盖。本版上板正常驱动不主动破坏目标通道；若日后专门安排故障验证，必须使用可恢复的测试环境和独立操作者，检查 FAULT 后零重试并重新建立 session，不修改目标使能/权限来恢复读能力。

核心来源：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
