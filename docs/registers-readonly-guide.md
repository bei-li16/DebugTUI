# 只读系统寄存器版本使用范围

本指南保留 `codex/register-debugging` 冻结 36 项 Goal 的范围。历史软件验收进度以 [唯一账本](registers-readonly-goal.md) 为准；后续正式版 1.1.3/1.1.4 已完成 STM32F429 的部分实板读取、状态及受控写入验收，当前范围见 [1.1.4 发布说明](release-1.1.4.md)。历史未执行硬件 case 仍为 SKIPPED，软件夹具、候选后端构建和目录中的声明都不代表对应硬件已通过。

## 支持边界

| 核 / 路径 | 本版范围 | 条件与限制 |
| --- | --- | --- |
| Cortex-M3 | SCB、故障字段、NVIC、SysTick、MPU、Debug、DWT/FPB 身份容量 | ID 决定实现/容量；未知 revision 布局保留字段缺口 |
| Cortex-M4 | M 公共目录、FPU 配置/身份及 DWT/FPB v1 详细定义 | FPU 数据复用 GDB regfile；不自动使能 FPU/DWT；FUNCTION 手动读取 |
| Cortex-M7 | M4 范围及 cache/TCM 配置、受保护 cache bank | CSSELR 保存恢复；维护命令不执行 |
| M7＋M4 / 同型多核 | 每核 CPU、目录、独立 PPB route、缓存及采样来源 | 相同 PPB 地址属于各自物理核，必须映射正确 AP/target |
| R52 原生只读协议 | 规定的 15 项身份/控制/MPU 标量、容量内 EL1/EL2 MPU region、MPU selector | 实际 Arm D13、AArch32、当前 Debug EL2、完整新鲜证明；不要求停止前程序处于 Hyp |
| R52 低 EL / Guest | 清晰的 Unknown / Access restricted | 不改模式、控制、trap 或权限来取得访问 |
| R52+ | 可选择定义目录 | 未适配身份保持 Unknown/Unsupported；不宣称 R52 适配适用于 R52+ |

完整 Banked 低 EL、VFP 写入、Bao/Guest 全权限、完整 Timer/PMU/GIC/STM、Trace、完整异常栈帧恢复，以及变量/内存/外设/sysreg 写入属于后续里程碑。历史开发文档保留相应代码的独立说明，不作为本版整类支持承诺。RO/RW 是寄存器访问属性；只有显式独立 writer 才可能提供编辑入口。本版只读事务允许必须恢复和回读的 scratch/selector 临时写入。

## 配置层和每核选择

工具 profile 提供工具路径、服务、端口和访问默认值，项目选择芯片/核心、ELF 及寄存器配置。根 `[registers]` 是项目默认，`[[cores]]` 下 `[cores.registers]` 可独立覆盖 CPU、目录和访问参数。省略字段继承；显式空 `cpu`/`catalogue` 清除对应选择。目录路径相对声明该字段的文件解析，未知键、非法命令、缺失或损坏目录在连接前报错。芯片拓扑、目录定义和 reader 能力是三个独立事实，芯片名不提供 CPU/AP 权限证明。

Setup 的 CPU / Catalogue 与 Status 展示各核有效值及来源。目录继承用公共定义加增量，重复 ID 必须显式 override；循环、过深、缺父及来源冲突拒绝加载。reset/source/confidence 缺少可靠资料时保持 Unknown，不补猜测值。见 [继承](register-catalogue-inheritance.md) 和 [策略](register-structured-policy.md)。

- [M7＋M4 完整项目模板](../profiles/registers-readonly-multicore.toml.example)：两个独立 GDB endpoint、两个核独占 PPB channel 和显式 owner。地址 `base=0` 使目录绝对 PPB 地址原样使用；不会推断 AP 编号。M3 可按已确认的核型号替换 CPU。
- [R52 双核完整项目模板](../profiles/registers-readonly-r52.toml.example)：普通读取 `aarch64 r52_read`、MPU selector `aarch64 r52_select`、`isb_command=""`，每个 core 名字须与 `registers.targets` 键匹配。endpoint/target 均是占位值，要以实际 OpenOCD CFG 核对。
- THA6 MCAL 项目仍使用 [项目模板](../profiles/tha6-project.toml.example) 和 [工具环境模板](../profiles/tha6-environment.toml.example)。其中板级 reset/启动策略与寄存器配置独立，不可把通用模板整份覆盖进已有 MCAL 工程。

M 的 CorePrivate 同地址读取必须走独立物理 route；`while_running=false` 是新模板默认。只有验证过允许运行 AP 读取的 channel 才显式设置 true。GDB memory fallback、regfile、CP15 和 selector/cache 事务仍要求暂停。NVIC 优先级位来源为 SVD 或显式配置，缺失 Unknown，不写 IPR 探测。DWT/FPU 未使能显示 NeedEnable，不自动改变 enable。

R52 原生配置需要 [固定修改后端](../third_party/openocd-adapter/README.md) 和独立 core/selector 协议。stock xPack、同名命令或 OpenOCD 版本号不能证明等价。每次事务以外部 MIDR/EDSCR 及 HALT、完整 DSPSR/DLR、scratch 恢复回读和当前容量判断；保存的 CPSR/DSPSR 只说明停止前程序状态。新 MPU selector 使用真正 ISB，不依赖或打开 CP15BEN；它不支持 PMU selector。旧 MRC/MCR 配置继续兼容，旧路径的权限限制不能作为新路径已验收的依据。细节见 [R52 受保护访问](register-r52-core-read.md)。

## 使用与自动化

在 System Regs 按需展开组，查看 Name/Value/Size/Access、字段/枚举、说明、格式和 Status。先显式 **Probe caps** / `:register-probe`，再 **Read**；**Read bank** / `:register-bank-read` 用于受保护的 selector。**MPU regions** / `:mpu el1` / `:mpu el2` 只在显式 Read 时采样，浏览缓存、搜索、进制和 Status 不读取目标。

自动化入口 `debugtui --project FILE --headless --stdio` 使用逐行 JSON 请求。离线可调用以下方法检查选择与路由，不发送 `connect`：

```json
{"id":1,"method":"select_core","params":{"name":"m7"}}
{"id":2,"method":"registers_list","params":{}}
{"id":3,"method":"registers_matrix","params":{}}
{"id":4,"method":"status","params":{}}
{"id":5,"method":"quit","params":{}}
```

实际读取方法为 `registers_probe`、`registers_read`、`registers_select` 和 `registers_mpu`；从当前返回结果获取 context，不手工复用旧 session/stop/frame。Scope All 的寄存器动作仍只读所选物理核。owner、线程/帧、运行/暂停、重连或路线变化撤销不适用缓存。

Not implemented、Reader unsupported、Access restricted、Unknown、Read error 和 stale 分别展示；失败不是数值 0。保留旧值时，其时间、实际命令/target/endpoint、owner 和权限依据都仍属于原采样，新的失败不能替换旧来源。恢复不确定会隔离通道并进入 FAULT；完成显式环境检查与重连后再访问，不自动换后端重试。

DHCSR/SysTick CTRL 等读副作用项不自动轮询，WO 不读，未知可选实现不自动猜测。MPU/cache 的临时选择器须恢复并回读；不完整或取消的批次不发布部分成功值。软件验证、延后硬件操作及恢复步骤见 [本版用例入口](../tests/cases/registers-readonly-release.md)。

## 三个核心参考（绝对路径）

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

指定的 `Armv8-R AArch32.pdf` 是架构介绍；详细 External Debug 规则另使用 TRM 引用的 `G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf`（ARM DDI 0568A.c）。目录说明保留手册矛盾、未知复位值与来源可信度，不把正常执行 EL 描述直接当作 Debug 授权。
