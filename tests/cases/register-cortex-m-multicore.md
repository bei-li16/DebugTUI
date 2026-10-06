# M7＋M4 同 PPB 地址环境验收

所有真实环境 case 当前为 **SKIPPED**；本 Goal 不执行上板测试。软件 fixture 只证明生产 Coordinator/MI/TCP/Tcl 的分派、控制流程和数据边界。`ap`、型号与 revision 的独立基线必须由芯片资料、专用固件或另一调试器取得，不能复制 DebugTUI 本次响应作为期望值。

准备专用测试固件与客户配置副本。记录两核型号/revision、探针、OpenOCD/GDB/DebugTUI 版本/提交、每核 AP/target/endpoint、启动脚本及实际连接图。初始两核 STOPPED、frame 0、MPU/selector 可访问；禁止在用户工作负载中执行运行控制或故障注入。保留所有配置 SHA256、MI/Tcl 日志、JSONL、独立 region 基线和报告。每核 PPB binding 必须对应自己的明确 channel 和 target。

| ID | 操作与独立预期 | 状态 |
| --- | --- | --- |
| M-MULTI-H01-IDENTITY | 同一进程选择 M7/M4，Scope All 下逐核 Probe；实际 CPUID/MPU 容量与独立基线相符，样本 owner、context、target、endpoint 属于当前核，没有广播到 peer | SKIPPED |
| M-MULTI-H02-VALUES | 对每核全部实际 MPU region 读取；每项 RBAR/RASR 与专用固件基线一致，原 selector 和恢复值一致；另一核原 selector、CTRL、region 内容不变 | SKIPPED |
| M-MULTI-H03-CACHED-SWITCH | M7→M4→M7→M4 查询已采样视图；只返回对应核原样本/context/来源，不复用另一核容量或同地址的值。结合 Tcl 日志核对 cached 查询零数据请求 | SKIPPED |
| M-MULTI-H04-RESTORE | 两核读回 RNR/CTRL，核对独立初值及配置 SHA256；仅 RNR 的临时选择/恢复写入，无 CTRL 写、维护命令、自动使能或运行控制 | SKIPPED |
| M-MULTI-H05-CACHE | 使用既有 M7 cache 驱动和独立 cache 基线；同一配置选择 M4 后 cache 读被拒绝，零 CSSELR/CCSIDR I/O。回到 M7 缓存仍属原核 | SKIPPED |
| M-MULTI-H06-LIFECYCLE | 专用固件中设 scope=core，M7 换帧/继续/暂停；M7 旧 MPU/cache stale 且保留原来源，M4 STOPPED 缓存不变。重连后两核旧会话不可用，重新 Probe 才恢复 indexed 读取 | SKIPPED |
| M-MULTI-H07-FAILURE | 优先用本地服务 fixture 验证单核已知错误/取消；恢复成功后该核重新 Probe，独立服务 peer 缓存不变。实板故障注入另需专用恢复方案，恢复不确定必须隔离、不得重试 | SKIPPED |
| M-MULTI-H08-IDENTITY-CHANGE | 离线配置副本故意给某核选错 CPU/目录；连接后型号冲突可见，该核无法借用另一核能力。原始基线与配置恢复后重新建立会话/Probe；不在运行中更改目标权限或控制寄存器 | SKIPPED |

默认驱动只生成四阶段 SKIPPED 报告，不启动 EXE，不连接目标：

```powershell
node scripts/test-m-profile-multicore-hardware.cjs
```

环境允许后先复制 `tests/fixtures/m-profile-multicore-board.example.json`。样例值仅为软件 fixture 常量；替换 metadata、每核 AP/target/endpoint、CPU/CPUID、容量、RNR/CTRL 和完整 region 列表，并将 `software_example` 改为 false。`ap` 是声明，必须独立核对实际 OpenOCD target 绑定，驱动不将其冒充观测证据。

```powershell
node scripts/test-m-profile-multicore-hardware.cjs --run --binary <开发EXE绝对路径> --project <专用配置副本绝对路径> --case <独立期望JSON绝对路径>
```

驱动复用已有 headless Session，执行 H01～H04，另外记录 cleanup；任一前置阶段失败时后续阶段 SKIPPED，错误身份基线在 MPU selector 写入前停止。报告保存 case/EXE/project SHA256、实际 owner/route/context、全部 region 和控制回读。`--software-fixture` 仅用于本地模型验证，报告 `board_tests_executed=false`，不能作为实板验收。

H05 使用已有 `scripts/test-m7-cache-hardware.cjs` 和独立 cache case；H06～H08 按表中步骤单独记录 JSONL 和日志。结束时关闭专用会话；出现恢复不确定先保留诊断和隔离状态，按板级恢复流程检查/复位专用固件后重连。还原配置副本，核对 SHA256，不改客户原文件，不升级 hardware verified。
