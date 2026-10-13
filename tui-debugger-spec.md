# 轻量 TUI 调试器 开发方案与需求清单

版本 v0.4（草案）｜读者：开发者与 AI 编码助手

> 修订记录：v0.2 补充三层配置（chip / 核型号 / debug 架构）的解析与覆盖规则、debug_arch 参数文件规范（5.1、5.6、5.7、8.6、PACK-09~13），并修正 8.3 中总线 AP 的角色示例。v0.3 补充 Cortex-M 核内寄存器的地址与差异来源（5.8）、核型号文件继承、M 核系统寄存器需求（6.12 MREG-01~11）及样例（8.7），并修正 DHCSR 读副作用与 NVIC 优先级位数的处理方式。v0.4 新增描述文件生成流程（10.4 重写）、FR-GEN（6.13），RegDef 增加来源与可信度字段。

---

## 0. 如何使用本文档

- 需求编号：`FR-` 功能需求，`NFR-` 非功能需求。
- 优先级：**P0** = MVP 必须；**P1** = 重要；**P2** = 可选。
- 标记 **【待验证】** 的条目依赖真实硬件或工具实测，**不得凭记忆实现**，必须先验证再编码（见第 11 章）。
- AI 开发约定与禁止项见第 13 章，开始编码前先读。

---

## 1. 项目定位

### 1.1 目标
做一个运行在 Windows 终端的**轻量调试看板**，底层复用现有的 **gdb + OpenOCD**，只补它们没有的部分：

- 芯片定制视图（寄存器、外设、内核系统寄存器）
- 运行时 Watch（内存、变量、外设）
- 多核、多 AP 的归属管理
- 个性化 skin（主题、布局、键位）

支持目标：Cortex-M（STM32 F1/F4/F7 起步）、Cortex-R（R52，如 NXP S32Z）；探针：CMSIS-DAP、J-Link（均经 OpenOCD）。

### 1.2 非目标
- 不重写 SWD/JTAG 协议、Flash 算法、探针驱动（交给 OpenOCD）。
- 不自己实现源码级调试（断点、单步、回溯、局部变量交给 gdb）。
- 不内嵌脚本引擎；复杂前置动作交给 Tcl 或 gdb 脚本。
- 不做 IDE 功能（编辑、构建、工程管理）。

---

## 2. 设计原则

1. **轻量**：TUI 本体空闲 CPU≈0，内存以几 MB 到十几 MB 为目标。
2. **事件驱动**：没有事件就阻塞，不使用固定 tick 空转。
3. **数据驱动**：芯片、核、外设差异放进配置文件，算法只写一份。
4. **单一控制权**：gdb 在线时，运行控制只经 gdb，TUI 不另发 halt/resume。
5. **不猜测**：读不到就显示 `n/a` 或 `--`，**绝不显示 0**；不确定的 scope、字段、编码标 `unknown`/`待核实`，不编造。
6. **不重造底层**：能用 OpenOCD/gdb 现成命令的，不自己实现。

---

## 3. 总体架构

```
                ┌──────────────── TUI（Rust）────────────────┐
                │  UI 线程（唯一重绘者）                      │
                │     ▲ mpsc                                  │
                │  ┌──┴────────── 事件汇聚 ────────────┐      │
                │  │ 输入线程  Tcl 线程  MI 线程  文件线程 │      │
                └──┴──────────┬──────────┬─────────────┴──────┘
                        Tcl 6666│        │GDB/MI（子进程管道）
                              ▼          ▼
                          OpenOCD ◄─RSP 3333─ gdb（按需启动）
                              │
                    CMSIS-DAP / J-Link ─► SWD/JTAG ─► 目标芯片
```

### 3.1 线程模型
- **UI 线程**：阻塞在 `rx.recv()`，有脏标记才重绘，上限约 30 FPS；突发数据先 `try_recv` 合并。
- **输入线程**：crossterm 事件，**必须过滤 `KeyEventKind::Press`**（Windows 会同时上报按下和松开）。
- **Tcl 线程**：独占与 OpenOCD 的 Tcl 连接，维护命令队列；用户命令优先级高于轮询；halt 等待、烧录期间暂停轮询。
- **MI 线程**：读取 gdb 子进程 stdout，解析 MI 输出为事件。
- 默认**不引入 tokio**，用标准线程加 `mpsc`。若后续证明必要，需在文档里记录理由。
- **空闲规则**：没有任何启用的轮询项时，Tcl 线程用无超时的 `recv()`；有轮询项时用 `recv_timeout(到期时间)`。

### 3.2 控制权规则
| 状态 | 运行控制（run/halt/step/reset） | 读内存/寄存器 |
|---|---|---|
| gdb 在线 | 只经 gdb（复位用 `monitor reset halt`） | Tcl 通道只读 |
| 无 gdb（退化模式） | TUI 经 Tcl 发 | Tcl |

所有 Tcl 命令写成 `<target> <cmd>` 形式，**不依赖 `targets <name>` 切换当前 target**（它是 OpenOCD 全局状态，会与 gdb 互相干扰）。

---

## 4. 技术选型

| 项 | 选择 | 备注 |
|---|---|---|
| 语言 | Rust | 无运行时、无 GC |
| TUI | ratatui + crossterm | 原生支持 Windows 控制台 |
| 配置 | serde + toml | 运行时加载 |
| SVD | svd-parser（或自写子集） | **按需解析**，只保留当前打开的外设 |
| ELF | `object` crate | 仅读符号表，不加载完整 DWARF |
| 进程管理 | std::process + Windows Job Object（windows-sys） | 保证退出时不留孤儿 OpenOCD/gdb |
| 调试后端 | OpenOCD（Tcl RPC + gdb RSP） | Tcl 命令以 `0x1a` 结尾 |
| 源码级调试 | gdb（`--interpreter=mi3`） | 管道通信，不需要 pty |

Release 配置：

```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

> 使用 `panic = "abort"` 时必须设置 panic hook，退出前恢复终端（关闭 raw mode 和备用屏幕），否则终端会残留乱码。

---

## 5. 核心概念与数据模型

### 5.1 三层配置
```
chip 文件      拓扑：核数、各核型号、AP 与 dbgbase、总线视图、SVD 路径、OpenOCD 参数
 └─ core 型号   r52.toml / cortex-m4.toml：寄存器列表、编码、复位值、字段、可用性；声明默认的 debug 架构
     └─ debug 架构  armv7m | armv8m | armv8r_a32：借核执行指令的算法参数
```

| 层 | 文件 | 谁来写 | 复用范围 | 性质 |
|---|---|---|---|---|
| chip | `packs/<chip>/chip.toml` | 适配芯片的人 | 单颗芯片 | 数据：拓扑与连接参数 |
| core 型号 | `regdefs/<core>.toml` | 由 TRM 生成并复核 | 所有使用该核的芯片 | 数据：寄存器定义 |
| debug 架构 | `debug_arch/<arch>.toml` + 代码中的 impl | 随程序发布 | 同一调试架构的所有核型号 | 参数（数据）+ 算法（代码，只写一份） |

解析与覆盖规则见 5.6，debug_arch 参数文件规范见 5.7，核型号文件之间的继承及 M 核寄存器的地址来源见 5.8。

- 核描述按**核型号**准备，不按芯片，可跨芯片复用。
- 外设描述用厂商 SVD，按芯片准备。
- 没有描述文件时仍可调试（halt/run/step/断点/烧录/hex/raw 模式），只是看不到寄存器名和位域。
- Cortex-M 的 SCS/DWT/FPB/NVIC 等架构寄存器**内置**随程序打包（`armv7m-common` 加核型号增量文件，见 5.8），开箱可看。

### 5.2 寄存器模型

```rust
enum Scope  { Core, Cluster, Chip, Unknown }

enum Access {
    Mmio   { base: BaseRef, off: u32 },        // BaseRef: CoreDbg | Bus(name) | PerCore{base, stride} | CorePrivate（M 核 PPB：off 即绝对地址，必须经该核自己的 AP/target）
    Sysreg { cp: u8, opc1: u8, crn: u8, crm: u8, opc2: u8, is64: bool }, // AArch32
    // AArch64 另设 op0/op1/crn/crm/op2，需要时再加
}

enum Avail {
    NotImplemented,                 // 架构有，本核无
    Inaccessible(Reason),           // NeedHalt | NeedEl(u8) | Unpowered | Trapped | Locked | NeedEnable(寄存器.位)
    Ok,
}

struct RegDef {
    name: String, desc: String, width: u8, reset: Option<u64>,
    scope: Scope, access: Access,
    avail_rule: AvailRule,          // min_el, need_halt, feature_gate
    read_action: bool,              // 读有副作用（来自 SVD readAction 或手工标注，如 DHCSR、SysTick CSR）
    instances: Option<Instances>,   // 实例个数由 ID 寄存器字段决定（NVIC ISER/ICER、MPU region、比较器）
    select: Option<Selector>,       // 间接选择，如 MPU 的 PRSELR
    fields: Vec<Field>,             // 结构对齐 SVD：bit_offset, bit_width, access(ro/rw/w1s/w1c/rc), enums
    group: Group,                   // Core | Simd | System（仅展示分组，与 Avail 独立）
    meta: RegMeta,                  // source{doc,version,page} + confidence（见 10.4.4）
}
```

- 显示、diff、位域高亮、Watch **只认 `RegDef`**，不关心它来自 SVD 还是核描述文件。
- 实例 key = `(归属实例, 名字)`；归属实例是某个核、某个 cluster 或 chip。**不能只用地址当 key**（同地址在不同视图含义不同）。

### 5.3 访问路径与能否运行时读

| 路径 | 例子 | 归属 | 运行时可读 |
|---|---|---|---|
| ① 核 sysreg（MRC/MCR） | SCTLR、MPU 的 PRBAR | per-core | 否，需 halt（借核执行） |
| ② 核调试 APB 内存映射 | EDSCR、PMU 的外部映射 | per-core | 核上电且无 OS lock 时可读 |
| ③ 系统总线内存映射 | GICD、外设、SRAM | chip/cluster | 是 |
| ④ 每核一帧的内存映射 | GICR（基址+核号×步长） | per-core | 是 |
| M 核 SCS/NVIC | 0xE000Exxx | per-core（同地址各核各一份） | 是（必须经该核自己的 AP/target；基址类型 `CorePrivate`） |

### 5.4 Watch 项

```rust
struct WatchItem {
    owner: Id,                 // 核 / 总线视图
    kind: Kind,                // Mem{addr,len} | Periph{addr,width} | CoreReg(id) | Reg(RegDef)
    enabled: bool,             // 默认 false
    interval_ms: u32,          // 默认 200
    side_effect: bool,         // 有读副作用：不进轮询，只允许手动读一次
    consistent: bool,          // 两次读取比对（防 torn read）
    last: Option<Vec<u8>>,
}
```

### 5.5 核寄存器与状态
- 核状态：`Running | Halted | Reset | Sleep | Lockup | Unpowered | Unexamined`。
- 核寄存器堆（R0–R15、xPSR 等）只在 **halted** 时读：检测到 halt 后一次性批量读全部已开启项并缓存，之后不再轮询；恢复运行即标记为过期（灰显）。
- 状态检测只在存在已开启的核寄存器项时进行。M 核优先用 OpenOCD 缓存的 `<target> curstate`（不占链路，也不干扰 OpenOCD 自己的状态跟踪）；**不要直接轮询 DHCSR**：读 DHCSR 会清除 S_RESET_ST、S_RETIRE_ST 粘滞位**【待核实，以 Arm 架构手册为准】**，会和 OpenOCD 自己的轮询冲突。R 核用 EDSCR（读副作用同样**【待核实】**）。

### 5.6 三层配置的解析与覆盖规则

**引用方式**：只用名字引用，不写路径。
- chip 文件 `[[cores]]` 的 `core = "cortex-r52"` → 加载 `regdefs/cortex-r52.toml`
- 核型号文件顶部的 `debug_arch = "armv8r_a32"` → 加载 `debug_arch/armv8r_a32.toml`
- 核数由 `[[cores]]` 的条目数决定，用户不需要另填 corenum；选定 chip 即确定每个核的型号。

**查找顺序**（先找到的生效，诊断页显示实际加载的路径）：① 当前 chip pack 目录内 → ② 用户配置目录 → ③ 随程序内置。

**覆盖优先级**：chip 文件 > 核型号文件 > debug_arch 文件默认值 > 内置默认值。

| chip 的 `[[cores]]` 字段 | 默认来源 | 说明 |
|---|---|---|
| `core` | 必填 | 决定加载哪份核型号文件 |
| `debug_arch` | 核型号文件的声明 | 仅在需要覆盖时才写 |
| `ap`、`dbgbase` | chip 文件 | `dbgbase` 优先由 ROM table 读出；chip 中显式写出的值视为用户覆盖 |
| `features` | 连接后用 ID 寄存器探测 | 显式声明视为覆盖，与探测结果冲突时告警 |
| `quirks` | 空 | 个别芯片的特殊处理标志 |
| `elf`、`svd` | 空 | 每核独立 |

**不允许在 chip 文件里覆盖寄存器定义**（编码、字段、复位值、可用性）。寄存器定义有误时修改核型号文件，避免某一颗芯片的特例悄悄改变其他芯片的行为。需要表达"本芯片没配置某特性"时用 `features`，由核型号文件里寄存器的 `feature_gate` 引用。

**校验时机**
- **加载时**：文件存在、字段合法、引用可解析；错误信息给出引用方文件、字段名和期望的文件名。
- **连接后**：MIDR（M 核为 CPUID）与核型号文件的期望值比对；MPIDR affinity 与 chip 文件里的核序号、cluster 划分比对；ROM table 读出的 dbgbase 与 chip 显式值比对。
- 任何不一致都**告警**（诊断页列出），不静默继续，也不自动改写文件。dbgbase 冲突时使用 chip 显式值，并在诊断页并列显示两个值。

**生效值可见**：诊断页对每个核列出最终生效的 core、debug_arch、ap、dbgbase、features、quirks，并标明每个值来自哪一层（chip / 核型号 / debug_arch / 内置 / ROM table / ID 寄存器探测）。

### 5.7 debug_arch 参数文件

debug 架构分两部分，**算法只在代码里写一份，差异只放在参数文件里**：

| 部分 | 位置 | 内容 |
|---|---|---|
| 实现（impl） | 代码 | 固定的访问流程。目前规划两种：`mmio_regfile`（M 核，经 DCRSR/DCRDR 读寄存器堆，sysreg 即内存映射）；`inject_a32`（R 核，借核执行 AArch32 指令：保存临时寄存器 → 注入指令 → 等待完成并检查错误 → 取回 → 恢复） |
| 参数（params） | `debug_arch/<name>.toml` | 调试寄存器位置、状态位、指令模板、超时与重试、能力标记 |

- 参数文件**不包含可执行序列，不引入脚本**：流程由 impl 固定实现，文件只提供数值与模板。
- 文件分组：`[meta]`（名称、impl、verified、来源）、`[regs]`（调试寄存器位置：M 核为绝对地址，R 核为相对 dbgbase 的偏移）、`[bits]`（状态位）、`[insn]`（指令模板）、`[limits]`（超时与重试）、`[caps]`（能力标记，驱动 UI 灰显与"需 halt"提示）。样例见 8.6。
- **`verified` 标记**：参数来自架构手册但尚未在板上验证时为 `false`。此时 UI 显示警告，并**禁止 sysreg 写入**；只有该文件在目标板上通过复位值自检（10.3）后，才由人手工改为 `true`。AI 不得自行改写。
- 同一 impl 下新增核型号或芯片，只需新增或复用参数文件；出现新的访问流程（如 AArch64 用 MRS/MSR 加 X0）才新增 impl。第二种 impl 等到有第二个实际实例时再抽象，不预先设计。
- 个别实现差异优先用 `quirks`，不复制 impl。

### 5.8 Cortex-M 核内寄存器：地址是否因厂商而异

**结论**：核内寄存器的地址由 Arm 架构定义，同一架构版本下各厂商一致；但"是否存在、有几个、哪些位有效"由核型号和厂商配置决定，另有一块厂商自己的区域。这些寄存器都位于私有外设总线（PPB，0xE0000000–0xE00FFFFF）。

按"由谁决定"分四类：

| 类别 | 例子 | 地址 | 有效性 | 处理方式 |
|---|---|---|---|---|
| ① 架构固定 | CPUID、ICSR、VTOR、AIRCR、CFSR/HFSR、SysTick、DHCSR/DCRSR/DEMCR | 同架构一致 | 同架构的核都有 | 写进 `armv7m-common` |
| ② 地址固定，数量或有效位取决于配置 | NVIC 中断数、MPU region 数、FPB/DWT 比较器数、FPU 是否配置 | 一致 | 连接后读 ID 寄存器确认 | `instances` + `feature_gate` |
| ③ 核型号特有 | M4 的 FPU 相关寄存器；M7 的 ITCMCR/DTCMCR/CACR 与 cache 维护寄存器；v8-M 的 SAU 与安全别名区 | 查对应核 TRM | 仅该型号有 | 核型号增量文件 |
| ④ 厂商专用 | STM32 的 DBGMCU（0xE0042xxx）及各家的调试冻结寄存器 | 厂商自定 | 仅该芯片有 | 放 chip pack（SVD 或 views），**不进核描述** |

**运行时确认配置**：连接后读下列 ID 寄存器，决定实例个数与 Avail，不按芯片型号查表。具体位域以 Arm 架构手册和各核 TRM 为准。

| 要确认的内容 | 读什么 | 备注 |
|---|---|---|
| 核型号 | CPUID.PARTNO | 与核型号文件的 `partno_expect` 比对（PACK-12） |
| 中断线数上限 | ICTR.INTLINESNUM | 只是以 32 为单位的上限；真实可用的中断以 SVD 的 interrupt 列表为准 |
| MPU region 数 | MPU_TYPE.DREGION | 为 0 表示未实现 MPU |
| FPB 比较器数 | FP_CTRL | 位域以手册为准 |
| DWT 比较器数 | DWT_CTRL.NUMCOMP | 需先使能 DEMCR.TRCENA |
| FPU 是否配置 | MVFR0 | 未配置时读为 0 或不存在**【待核实】** |

**NVIC 优先级位数不能从 ID 寄存器读出**：v7-M 没有直接给出实现位数的寄存器。来源依次为：① SVD `<cpu>` 的 `nvicPrioBits`；② chip 文件声明；③ 手动探测（向 IPR 写 0xFF 再读回，并**恢复原值**）：有写副作用，只在 halt 且用户确认后执行，默认关闭。都没有时显示 `unknown`，不猜测。

**数据来源**
- **权威来源**：Arm 架构手册（ARMv6-M / v7-M / v8-M）加各核 TRM。
- **实用来源**：CMSIS 核头文件（`core_cm3.h`、`core_cm4.h`、`core_cm7.h`、`core_armv8mml.h` 等），结构体偏移和位掩码是现成的，用脚本转成 toml（Apache-2.0，保留声明）。
- **厂商 SVD**：以外设为主，是否包含核内寄存器因厂商而异，**不依赖它**。
- 转换结果用复位值自检（10.3）校验。复位值因实现而异的寄存器不填 `reset`，不参与自检。

**核型号文件继承**：核型号文件用 `extends = ["armv7m-common"]` 继承公共部分，只写差异。
- `armv7m-common` 定义 SCB、NVIC、SysTick、MPU、DWT、FPB、调试寄存器；`cortex-m3/m4/m7` 只写增量（M4：FPU；M7：cache 与 TCM 控制）。
- 同名寄存器在子层重定义必须显式 `override = true`，否则加载报错；继承链不得有环，深度建议不超过 3。
- `optional_features` 取并集；合并后的结果在诊断页可见，每个寄存器标明来自哪个文件。
- 特性存在性条件使用结构化写法（`present_if = { reg, field, op, value }`），**不引入表达式语言或脚本**。

**多核 M 的归属**：同一个 0xE000Exxx 地址，每个核各有一份（如 H745 的 M7 与 M4）。这类寄存器统一使用 `CorePrivate`，必须经该核自己的 AP 或 target 访问，Watch 的 key 带核归属。

---

## 6. 功能需求

### 6.1 连接与进程管理（FR-CONN）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| CONN-01 | 根据 chip + probe profile 拼装命令行并启动 OpenOCD 子进程，stdout/stderr 进入日志环形缓冲 | P0 | 选定 profile 后一键连接；日志页可见 OpenOCD 输出 |
| CONN-02 | attach 模式：填 `host:port` 连接已运行的 OpenOCD，不启动、不终止它 | P0 | TUI 退出后外部 OpenOCD 仍在运行 |
| CONN-03 | Tcl 客户端：单连接，命令以 `0x1a` 结尾，命令队列，用户命令优先于轮询 | P0 | 轮询进行中发出的用户命令在下一个调度点即执行 |
| CONN-04 | 退出或崩溃时清理子进程（Windows Job Object） | P0 | 任务管理器中无残留 openocd/gdb |
| CONN-05 | 断线检测，有限次退避重连；重连后恢复 Watch 清单，核寄存器标过期 | P1 | 拔插探针后可自动恢复或明确提示失败 |
| CONN-06 | 原始 Tcl 命令行直通 | P1 | 可输入任意 Tcl 并显示返回 |
| CONN-07 | 端口冲突、探针未识别等常见错误给出可读提示 | P1 | 端口被占用时提示而非卡死 |
| CONN-08 | Tcl/gdb 端口只监听 127.0.0.1 | P0 | 启动参数包含 bind 限制（待验证具体命令） |

### 6.2 运行控制（FR-CTRL）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| CTRL-01 | halt / run / step | P0 | 状态栏随之更新 |
| CTRL-02 | 复位：halt 复位、run 复位、系统复位三种，UI 上区分命名（单核复位多数芯片做不到，不提供假选项） | P0 | 三种复位均可触发 |
| CTRL-03 | gdb 在线时运行控制全部经 gdb（见 3.2） | P0 | gdb 在线时 TUI 不发 Tcl 的 resume/halt |
| CTRL-04 | 断点/观察点管理，显示硬件资源余量（FPB/DWT 比较器数量因芯片而异） | P1 | 超限时给出提示 |
| CTRL-05 | 烧录：`program` + 校验 + 复位；RDP/读保护状态提示 | P1 | 烧录期间暂停轮询，完成后恢复 |

### 6.3 内存与定位（FR-MEM）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| MEM-01 | hex 视图：按地址显示，宽度 8/16/32 位，滚动，**只读取可见区域** | P0 | 翻页不会读整个 Flash |
| MEM-02 | 读失败（保护区、未上电）显示 `--` 并给原因，不显示 0 | P0 | 访问非法地址不崩溃 |
| MEM-03 | 地址跳转、书签、符号名跳转 | P1 | `main`、`0x2000_0000` 均可跳转 |
| MEM-04 | 符号反查：地址旁显示 `函数+偏移`（仅 ELF 符号表） | P1 | 内存占用不因此显著增加 |
| MEM-05 | 快照对比、搜索 | P1 | 两次快照差异高亮 |
| MEM-06 | dump 到文件 / 从文件加载 | P2 | — |
| MEM-07 | 调用栈回溯（无 gdb 时基于 EXC_RETURN 与栈帧，简版） | P2 | 有 gdb 时用 gdb 的结果 |

### 6.4 寄存器与 Watch（FR-REG / FR-WATCH）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| REG-01 | 统一 `RegDef` 模型；显示/diff/位域解码与来源无关 | P0 | SVD 外设与内置 SCS 用同一套组件显示 |
| REG-02 | 位域解码、变化高亮（值未变不触发重绘） | P0 | — |
| REG-03 | Avail 三层显示：`n/a`（灰、默认折叠）、`--（需 halt/需 EL2…）`、正常值 | P0 | 任何情况下不出现伪造的 0 |
| REG-04 | 读副作用保护：`side_effect` 项不进轮询，只提供"读一次"按钮；SVD 的 readAction 自动标记 | P0 | USART_DR 一类不会被自动读 |
| REG-05 | 写入：默认只读；字段级写入需确认；W1C 字段不做读改写 | P1 | 写操作记入操作日志 |
| REG-06 | 寄存器树：Chip / Core0… / 外设；标记 `[H]`需 halt、`[RT]`运行可读、`[!]`有副作用 | P1 | — |
| REG-07 | 间接选择寄存器（"写选择器→读 BAR/LAR→恢复选择器"），MPU 做成专用表格 | P2 | 批量读完恢复选择器 |
| REG-08 | raw 模式：手动输入地址或 sysreg 编码读写（新核未有描述文件时用于验证通路） | P1 | — |
| REG-09 | 跨核对比视图（仅 per-core 寄存器） | P2 | MPU/SCTLR 各核差异一眼可见 |
| WATCH-01 | 开关式 Watch，默认全关；只有开启的项才产生读操作 | P0 | 全关时用抓包或日志确认零读操作 |
| WATCH-02 | 调度：收集到期项，相邻地址合并成块读（Tcl 用 `read_memory addr width count`），一轮一次往返 | P0 | 多个相邻变量只产生一次往返 |
| WATCH-03 | 面板不可见时暂停该面板的轮询 | P0 | 切页后读操作停止 |
| WATCH-04 | 读失败自动关闭该项并在状态栏提示，不无限重试 | P0 | — |
| WATCH-05 | 一致性读（两次比对）选项，用于 64 位/结构体/共享 RAM | P1 | — |
| WATCH-06 | 数值 sparkline、CSV 导出、触发条件（超过阈值自动 halt，halt 经 gdb/控制权规则） | P2 | — |
| WATCH-07 | 变量名解析：从**该核自己的 ELF** 符号表取地址 | P1 | 多核各自 ELF 不混用 |

### 6.5 核调试后端（FR-CORE）

```rust
trait CoreDebug {
    fn state(&mut self) -> CoreState;
    fn read_regfile(&mut self) -> Result<RegSnapshot, Err>;      // halt 后批量读
    fn read_sysreg(&mut self, r: &RegDef) -> Result<u64, Err>;
    fn write_sysreg(&mut self, r: &RegDef, v: u64) -> Result<(), Err>;
    fn fault_info(&mut self) -> Option<FaultInfo>;
}
// 实现：CortexM（全 Mmio，无需借核）、ArmV8R（借核执行）
```

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| CORE-01 | `CoreDebug` trait 及 Cortex-M 实现（DHCSR/DCRSR/DCRDR/SCS 均为内存映射；状态检测用 `curstate`，不轮询 DHCSR） | P0 | F1/F4/F7 上能读寄存器堆、SCS、故障寄存器 |
| CORE-02 | Cortex-M 故障分析：CFSR/HFSR/MMFAR/BFAR + 异常压栈帧，PC/LR 反查函数名 | P0 | 人为制造 HardFault 能定位到函数 |
| CORE-03 | Cortex-M 运行态观测：DWT_CYCCNT（需先置位 DEMCR.TRCENA 与 DWT_CTRL.CYCCNTENA）；DHCSR 状态位仅手动读一次（读会清粘滞位） | P1 | — |
| CORE-04 | ARMv8-R（AArch32）后端：借核执行 MRC/MCR（EDITR/EDSCR/DTR）**【待验证】** | P1 | 复位值自检通过（见 10.3） |
| CORE-05 | 借核序列在一次事务内完成：保存 R0 → 批量 MRC/MRRC → 恢复 R0 与相关状态；期间暂停其他命令 | P1 | 操作前后 gdb 看到的寄存器一致 |
| CORE-06 | 错误分类：链路错误 / 核报 UNDEFINED（寄存器不存在或被陷阱）/ 核掉电 / OS lock；sticky 错误标志读后清除 | P1 | Avail 判定与实际一致 |
| CORE-07 | `sysreg_backend = "auto"`：先尝试 OpenOCD 的 `arm mrc`，失败自动切借核；同一 trait 后面可换 **【待验证】** | P2 | — |
| CORE-08 | F7 提示：D-Cache 开启 write-back 时调试器读 RAM 可能偏旧（profile 中 `dcache=true` 标记） | P1 | UI 有提示 |
| CORE-09 | 核型号识别：M 读 CPUID，R 以 chip 文件为准、连接后读 MIDR 复核，不一致告警（比对规则见 PACK-12） | P1 | 选错核型号文件时告警 |

> 编码公式：Sysreg 到 MRC/MCR 机器码是通用公式，寄存器列表只是输入。EDITR/EDSCR/DTR 偏移及位定义是 `debug_arch` 参数文件的内容（见 5.7、8.6），**必须对照 Arm ARM（External Debug 章节）和对应核 TRM**，不得凭记忆写入代码。

### 6.6 多核与 AP（FR-MULTI）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| MULTI-01 | 连接后发 `targets` 枚举，与 profile 的 `cores`/`views` 匹配；未匹配的显示为"未知 target"，不丢弃 | P1 | — |
| MULTI-02 | 状态栏一行显示所有核状态；Tab 切换当前核 | P1 | — |
| MULTI-03 | 每个 Watch/寄存器带归属（核或总线视图） | P1 | 切核只影响 per-core 内容 |
| MULTI-04 | 全停/全跑：写成一条 `foreach` 脚本一次发出；需要硬同步才用 CTI | P1 | 核间时间差只剩传输延迟 |
| MULTI-05 | 每核独立 ELF 和 SVD；gdb 端口映射表（OpenOCD 按 target 顺序递增分配） | P1 | — |
| MULTI-06 | 未 examine 的核（`-defer-examine`）显示 `unexamined`，提供手动 `arp_examine`，复位后自动重试一次 | P1 | — |
| MULTI-07 | 总线视图（`mem_ap`）：Watch 默认走"能读到该地址的总线视图"，失败自动换下一路径；可手动指定（`via` 优先级列表） | P1 | — |
| MULTI-08 | AP 诊断页：扫描 AP 0–7，显示 IDR 类型、ROM table 组件；对每个 AP 测试读 Flash/SRAM 基址；给出角色建议（`core`/`bus`/`debug`，可多角色）；用户确认后写回 profile | P1 | 读到 IDR=0 的 AP 自动跳过 |
| MULTI-09 | 角色判定规则：非 MEM-AP 忽略；ROM table 含内核调试组件（M: SCS，A/R: Debug 单元）→ core；AHB/AXI 且 SRAM 可读 → bus；APB 且只有调试组件 → debug。Cortex-M 的 AP 通常是 `core+bus`。**仅为建议，以参考手册为准** | P1 | — |
| MULTI-10 | `dap info`、`apid`、`baseaddr` 的输出措辞随 OpenOCD 版本变化：IDR 用数值自行解码，ROM table 文本按关键词匹配，不按整行匹配 **【待验证】** | P1 | — |

### 6.7 gdb 集成（FR-GDB）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| GDB-01 | 按需启动 `gdb --interpreter=mi3`，不使用源码调试时不启动 | P0 | 未启动时 TUI 内存不含 gdb |
| GDB-02 | MI 解析：停止原因、断点命中、异步通知 | P0 | — |
| GDB-03 | 断点增删、continue/step/next/finish、`monitor reset halt` | P0 | — |
| GDB-04 | 调用栈、局部变量（`-stack-list-frames`、`-stack-list-variables`） | P1 | — |
| GDB-05 | 轻量源码视图：MI 返回文件和行号，TUI 从磁盘读源文件，只显示当前行上下文 | P1 | 不加载 DWARF |
| GDB-06 | gdb 的 target description 被解析显示：r0–r15 和 cpsr 来自 stub 的 `target.xml`（可用 `qXfer:features:read:target.xml` 或 `maint print target-description` 查看）；若 stub 提供更多寄存器组，按 group 分成 Core/SIMD/System 自动显示 | P1 | stub 补充寄存器后界面自动出现 |
| GDB-07 | 可选：RSP 代理注入自定义寄存器，使 gdb 的 `info registers` 也能看到 sysreg | P2 | — |

### 6.8 运行时观测（FR-OBS）

| ID | 需求 | 优先级 |
|---|---|---|
| OBS-01 | RTT 日志（OpenOCD `rtt` 命令，通过 Tcl 或 RTT 端口读取）**【待验证命令与版本】** | P1 |
| OBS-02 | SWO/ITM（需接线与 TPIU 配置，CMSIS-DAP 与 J-Link 支持差异大） | P2 |
| OBS-03 | 栈水位检测：启动时填魔数，定期扫描最大使用深度，纯内存读 | P2 |
| OBS-04 | RTOS 任务列表（OpenOCD `-rtos`，需固件带对应符号） | P2 |
| OBS-05 | DWT PC 采样 / profile 热点（睡眠或 halt 时 PCSR 可能无效） | P2 |
| OBS-06 | Semihosting、串口终端 | P2 |

### 6.9 芯片包与描述文件（FR-PACK）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PACK-01 | chip pack 目录加载（见附录 A.1） | P0 | 新增芯片只需新增目录 |
| PACK-02 | 外设 SVD 加载，转换成 `RegDef`；**按需解析**，只保留当前打开的外设 | P0 | 加载 SVD 不使常驻内存显著增长 |
| PACK-03 | 核描述文件加载（`regdefs/<core>.toml`） | P1 | — |
| PACK-04 | 自定义面板：声明式 toml（寄存器列表、字段格式、分组、刷新规则） | P1 | 不写代码即可新增面板 |
| PACK-05 | `pre_attach` 步骤列表：用 `mww/mdw` 等同一套机制执行，如"写寄存器→等状态位"（核上电、解除 OS lock、调试授权，S32Z 等 SoC 需要） | P1 | 失败时给出可读提示 |
| PACK-06 | `init.tcl` 作为芯片专用前置脚本交给 OpenOCD | P1 | — |
| PACK-07 | `quirks = [...]` 标志，算法内留少量分支，**不为每颗芯片复制算法** | P2 | — |
| PACK-08 | 配置加载时做校验，错误指出文件与字段 | P0 | — |
| PACK-09 | 三层解析：chip → 核型号 → debug_arch，按名字引用；查找顺序 pack 目录 → 用户目录 → 内置；引用缺失时报错，给出引用方文件、字段和期望文件名 | P0 | 删除任一被引用文件，错误信息可定位到引用处 |
| PACK-10 | 覆盖规则（见 5.6）：chip 的 `[[cores]]` 可覆盖 `debug_arch/dbgbase/ap/features/quirks`，**不可覆盖寄存器定义**；诊断页显示每个生效值的来源层 | P0 | 覆盖 debug_arch 后算法随之切换；覆盖寄存器定义被拒绝并报错 |
| PACK-11 | debug_arch 参数文件加载与校验：`impl` 必须是已编译实现；必填字段缺失、未知字段、位号越界报错；`verified=false` 时 UI 警告并禁止 sysreg 写入 | P1 | 未验证文件下写 sysreg 被拒绝并提示原因 |
| PACK-12 | 连接后一致性校验：MIDR/CPUID、MPIDR affinity、ROM table 的 dbgbase 与配置比对；不一致告警，不静默继续 | P1 | 故意选错核型号文件时诊断页告警 |
| PACK-13 | `features`：chip 可声明可选特性（如 `fpu_simd`），缺省用 ID 寄存器探测，冲突告警；核型号文件的寄存器通过 `feature_gate` 引用 | P2 | 关闭特性后相关寄存器显示 `n/a`（NotImplemented） |

### 6.10 界面与 skin（FR-UI）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| UI-01 | 语义色主题：`fg bg accent changed stale error running halted border selected` 等角色，组件只引用角色名 | P0 | 换 `theme.toml` 不改代码 |
| UI-02 | 边框样式（plain/rounded/double/ascii）与状态图标（带 ASCII 回退） | P1 | — |
| UI-03 | 颜色降级：`truecolor / 256 / ansi` 自动检测，`ansi` 模式使用终端自带调色板；兼容 Windows Terminal 与旧 conhost | P1 | 旧 conhost 下可读 |
| UI-04 | 主题热加载（按键重载） | P1 | 改配色后不重启即生效 |
| UI-05 | 布局与键位放配置文件（分栏比例、面板排布、快捷键） | P1 | — |
| UI-06 | 命令面板（`:` 或 `Ctrl+P`）、快捷键帮助页 | P1 | — |
| UI-07 | 通用页（状态栏、Watch、寄存器树、hex）只依赖 `RegDef`/`CoreState`；少量专用页（故障分析、寄存器堆展示）由核型号配置指定，**不写 if-else 分支** | P0 | — |
| UI-08 | 能力标记（`caps`）驱动灰显、"需 halt"提示和 Avail 原因 | P1 | — |

### 6.11 会话与诊断（FR-SESSION）

| ID | 需求 | 优先级 |
|---|---|---|
| SESS-01 | session 保存与恢复：连接配置、Watch 清单（**恢复后默认全部关闭**）、布局、书签 | P1 |
| SESS-02 | 操作日志：写寄存器、烧录、复位等记录 | P1 |
| SESS-03 | 诊断页：AP 诊断、复位值自检结果、OpenOCD 日志 | P1 |
| SESS-04 | 批处理/无界面模式、脚本宏 | P2 |

### 6.12 M 核系统寄存器（FR-MREG）

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| MREG-01 | 内置 `armv7m-common` 核描述：SCB、NVIC、SysTick、MPU、DWT、FPB、调试寄存器；地址取自 Arm 架构手册 / CMSIS 核头文件，由 MREG-10 的脚本生成并复核 | P0 | F1/F4/F7 开箱可看，用户无需提供任何描述文件 |
| MREG-02 | 核型号文件继承（`extends`）：`cortex-m3/m4/m7` 只写增量；同名寄存器子层重定义必须显式 `override = true`；禁止环；诊断页显示每个寄存器来自哪个文件 | P0 | 改 `armv7m-common` 的一个寄存器，M3/M4/M7 同时生效；无 override 的重名报错 |
| MREG-03 | 基址类型 `CorePrivate`：绝对地址，必须经该核自己的 AP/target 访问；多核 M 同一地址每核一份，Watch key 带核归属 | P0 | 双核 M 上两个核读 CPUID 各得到自己的核型号 |
| MREG-04 | ID 寄存器驱动配置：连接后读 CPUID/ICTR/MPU_TYPE/FP_CTRL/DWT_CTRL/MVFR0，决定 `instances` 个数与 Avail（`NotImplemented`），不按芯片型号查表；ICTR 只给中断线数上限，真实中断以 SVD 为准 | P0 | 换 F1/F4/F7 板子无需改核描述，MPU region 数、NVIC 寄存器个数自动适配 |
| MREG-05 | 读副作用标注：DHCSR（读清 S_RESET_ST/S_RETIRE_ST）、SysTick CSR（读清 COUNTFLAG）等标 `read_action`，不进轮询，只手动读一次；M 核状态检测用 `curstate`，不轮询 DHCSR **【待验证】** | P0 | 开启 Watch 后 OpenOCD 仍能正常检测到复位事件 |
| MREG-06 | "需先使能"：DWT/ITM 依赖 `DEMCR.TRCENA`；Avail 原因新增 `NeedEnable(寄存器.位)`；UI 提示并提供一键使能（需确认，记入操作日志） | P1 | 未使能时显示"需先置位 DEMCR.TRCENA"，使能后可读 |
| MREG-07 | 写 1 置位/清零/读清类字段：NVIC 的 ISER/ICER/ISPR/ICPR 读回的是当前状态，写 1 才生效；字段 access 支持 `w1s/w1c/rc`；写入走 REG-05 确认，禁止读改写 | P1 | 写 ISER 不会误改其他位 |
| MREG-08 | 厂商专用寄存器（如 STM32 DBGMCU 0xE0042xxx）放 chip pack 的 SVD 或 views，不进核描述 | P1 | 核描述文件中不出现厂商地址 |
| MREG-09 | 核型号增量：M4 的 FPU（CPACR、FPCCR、FPCAR、FPDSCR、MVFR）；M7 的 cache/TCM（CLIDR、CTR、CCSIDR、CSSELR、CACR、ITCMCR、DTCMCR、AHBPCR）；v8-M 的 SAU 与安全别名区为 P2 | P1 | F7 上可见 cache/TCM 配置 |
| MREG-10 | 数据来源脚本 `tools/cmsis2regdefs`：从 CMSIS 核头文件生成 toml（保留 Apache-2.0 声明）；复位值因实现而异的寄存器不填 `reset`；不把厂商 SVD 当作核内寄存器的来源 | P0 | 生成结果通过复位值自检（10.3） |
| MREG-11 | NVIC 优先级位数：不假定可由 ID 寄存器读出。来源依次为 SVD `<cpu>` 的 `nvicPrioBits`、chip 文件声明；手动探测（写 IPR 读回并恢复原值）仅作为用户确认后的选项，默认关闭 | P1 | 无 SVD 且未声明时显示 `unknown`，不显示猜测值 |

### 6.13 描述文件生成流程（FR-GEN）

> 离线流程，不进 TUI。可以是脚本、AI 辅助，或两者结合；产出必须符合同一 schema，并通过同一套自动检查和板上自检。流程详见 10.4。

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| GEN-01 | TRM → 核寄存器描述（`regdefs/<core>.toml`）：提取名称、编码（MRC/MCR 五元组或内存映射地址）、位宽、位域、复位值、访问权限与最低 EL、读副作用、可用性条件；同时产出人工复核清单 | P0 | 给一章 TRM，产出 toml 片段和复核清单 |
| GEN-02 | 每条记录必须带 `source`（文档、版本、页码）；文件头带文档名、编号、版本、生成方式与时间；缺来源则校验报错 | P0 | 任一寄存器可追溯到手册页码 |
| GEN-03 | 缺失与矛盾的处理：缺失内容标 `fields_missing` 或 `TODO(待核实)`；正文与表格矛盾时两者都记录并标 `conflict`；**不凭记忆补全，不自行裁决** | P0 | 复核清单列出全部缺失与矛盾项 |
| GEN-04 | 自动检查（`tools/checkregs`）：编码唯一、位域不重叠不越界、复位值与位域一致、名称唯一、scope 与来源一致、source 必填 | P0 | 故意制造重叠位域或重复编码，检查能报出 |
| GEN-05 | 可信度分级 `confidence`（`low/medium/high/verified`）；TUI 诊断页汇总，`low/medium` 的条目显示标记 | P1 | 诊断页能按级别过滤寄存器 |
| GEN-06 | UM → `chip.toml` 草稿：提取内存映射（`mem` 表）、核数与核型号、cluster 划分、外设基址；AP 编号、dbgbase 留空，附手册线索，输出待填项清单 | P1 | 草稿加载不报错，待填项一目了然 |
| GEN-07 | 外设优先使用厂商 SVD；仅无 SVD 时才从 UM 的 PDF 提取，且整体标 `low` | P1 | 有 SVD 时流程不走 PDF 提取 |
| GEN-08 | 样例先行、分批生成：先出 3 到 5 个样例供确认格式，再按章节分批；每批独立产出、可追溯、可 diff；手册版本变更时重跑并输出 diff 报告 | P1 | 重跑后能看到新增、删除、变更的寄存器与字段 |
| GEN-09 | 扫描版 PDF：先 OCR，整体降级为 `low`，复核比例提高 | P2 | — |
| GEN-10 | `debug_arch` 草稿：从 TRM 与 Arm ARM 提取调试寄存器偏移、位号、指令模板；强制 `verified = false` | P1 | 生成文件不会出现 `verified = true` |
| GEN-11 | 板上复位值自检报告可从 TUI 导出为文件，回传给生成流程定位错误条目；自检通过的条目可升级为 `verified` | P1 | 失败项能对应到 toml 中的具体寄存器与来源页码 |

---

## 7. 非功能需求

| ID | 需求 | 目标（实测后修订） |
|---|---|---|
| NFR-01 | 空闲 CPU | 无轮询项时 ≈ 0% |
| NFR-02 | 内存 | TUI 本体常驻几 MB 到十几 MB（不含 OpenOCD、gdb）；记录基线并随版本回归 |
| NFR-03 | 体积 | 单个 exe，目标数 MB 级 |
| NFR-04 | 重绘 | 脏标记 + 上限约 30 FPS；ratatui 仅输出变化单元格 |
| NFR-05 | 链路预算 | 所有核与视图共用一条 SWD/JTAG，轮询在全局合并调度；SWD 速度默认 4 MHz 起（`adapter speed 4000`，按探针与线长调整） |
| NFR-06 | 环形缓冲 | 日志、曲线均为定长环形缓冲，只渲染可见行 |
| NFR-07 | 稳定性 | 任何读失败不得 panic；panic 时恢复终端 |
| NFR-08 | 平台 | Windows 10/11，Windows Terminal 与 conhost |
| NFR-09 | 驱动提示 | J-Link 给 OpenOCD 用需 Zadig 换 WinUSB（会影响 SEGGER 官方工具）；CMSIS-DAP v2 一般免驱，v1 走 HID |
| NFR-10 | 安全 | 端口只监听本机；所有写操作可追溯；高风险写需确认 |
| NFR-11 | 可测试 | 后端均可替换为 mock，无硬件即可跑 CI |

---

## 8. 配置文件规范（示例）

> 示例中的地址、AP 编号、编码仅作格式示意，具体值以芯片参考手册和 TRM 为准。

### 8.1 probe profile
```toml
# probes/jlink.toml
openocd_interface = "interface/jlink.cfg"
transport = "swd"
speed_khz = 4000
serial = ""          # 多探针时填
```

### 8.2 chip.toml（Cortex-M 单核）
```toml
name = "STM32F407VG"
openocd_target = "target/stm32f4x.cfg"
dcache = false
svd = "svd/STM32F407.svd"

[[cores]]
name = "cpu0"
target = "stm32f4x.cpu"
core = "cortex-m4"           # 选 regdefs/cortex-m4.toml；debug_arch 默认由它声明（armv7m）
elf = ""

[[mem]]
name = "FLASH"; base = 0x08000000; size = 0x100000; via = ["cpu0"]
[[mem]]
name = "SRAM";  base = 0x20000000; size = 0x20000;  via = ["cpu0"]
[[mem]]
name = "CCM";   base = 0x10000000; size = 0x10000;  via = ["cpu0"]
```

### 8.3 chip.toml（多核、多 AP，R52）
```toml
name = "example_r52_soc"

[[aps]]
num = 1
role = ["bus"]         # 角色是集合：core | bus | debug（此 AP 后面是 mem_ap 总线视图）
target = "bus0"        # mem_ap

[[cores]]
name = "rtu0_core0"
core = "cortex-r52"            # 选 regdefs/cortex-r52.toml，debug_arch 默认由它声明
# debug_arch = "armv8r_a32"    # 仅需覆盖核型号文件的默认值时才写
ap = 3
dbgbase = 0x00000000           # 优先从 ROM table 读，此处仅覆盖/校验【待核实】
features = { fpu_simd = false } # 可选特性示意；缺省时用 ID 寄存器探测

[[pre_attach]]                 # 用 mww/mdw 机制执行
action = "mww"; addr = 0x0; value = 0x0
[[pre_attach]]
action = "wait_bit"; addr = 0x0; mask = 0x1; timeout_ms = 100
```

### 8.4 核寄存器描述（由 TRM 生成）
```toml
# regdefs/cortex-r52.toml
core = "cortex-r52"
debug_arch = "armv8r_a32"      # 默认调试架构，chip 文件可覆盖
optional_features = ["fpu_simd"]  # 寄存器的 feature_gate 引用这些名字
midr_expect = "待核实"          # 用于型号复核

[meta]                           # GEN-02：文件头记录来源
doc = "Cortex-R52 TRM"
doc_id = "待填"
version = "待填"
generated_by = "待填"            # 脚本或 AI 流程及其版本
generated_at = "待填"

[[reg]]
name = "SCTLR"
desc = "System Control Register"
width = 32
scope = "core"
group = "system"
access = { type = "sysreg", cp = 15, opc1 = 0, crn = 1, crm = 0, opc2 = 0 }
avail = { min_el = 1, need_halt = true }
reset = 0x00000000             # 以 TRM 为准；字段缺失标 fields_missing = true
read_action = false
source = { section = "待填", page = 0 }   # 必填（GEN-02）
confidence = "medium"                      # low | medium | high | verified（见 10.4.4）
fields = [ { name = "M", bit = 0, width = 1, desc = "MPU enable" } ]
```

### 8.5 theme.toml
```toml
color_mode = "auto"   # auto | truecolor | 256 | ansi
border = "rounded"    # plain | rounded | double | ascii

[colors]
fg = "#d8dee9"; bg = "#1e222a"; accent = "#88c0d0"
changed = "#ebcb8b"; stale = "#616e88"; error = "#bf616a"
running = "#a3be8c"; halted = "#d08770"; border = "#434c5e"; selected = "#3b4252"

[icons]
running = "▶"; halted = "■"; fallback_ascii = true
```

### 8.6 debug_arch 参数文件

> `armv7m` 中的地址是 Cortex-M 架构定义的固定地址；`armv8r_a32` 中的数值**全部是占位**，必须对照 Arm ARM（External Debug 章节）和 R52 TRM 填写并在板上验证。`verified = false` 时 TUI 会警告并禁止 sysreg 写入。

```toml
# debug_arch/armv7m.toml
[meta]
name = "armv7m"
impl = "mmio_regfile"            # 代码里已有的实现名；未知名字加载失败
verified = true                  # 架构定义的固定地址
source = "Armv7-M Architecture Reference Manual, Debug 章节"

[regs]                           # M 核：绝对地址
dhcsr = 0xE000EDF0
dcrsr = 0xE000EDF4
dcrdr = 0xE000EDF8
demcr = 0xE000EDFC

[bits]
dhcsr_s_halt = 17

[caps]
borrow_core = false              # 不借核，sysreg 即内存映射
regfile_needs_halt = true
sysreg_via = "mmio"
```

```toml
# debug_arch/armv8r_a32.toml
[meta]
name = "armv8r_a32"
impl = "inject_a32"
verified = false                 # 板上复位值自检通过后，由人手工改为 true
source = "TODO: Arm ARM External Debug 章节号；R52 TRM 调试寄存器章节"

[regs]                           # 相对 dbgbase 的偏移，全部待核实
edscr = 0x000                    # TODO(待核实)
editr = 0x000                    # TODO(待核实)
dtrrx = 0x000                    # TODO(待核实)
dtrtx = 0x000                    # TODO(待核实)
# OS lock、锁寄存器等按 impl 需要补充

[bits]                           # EDSCR 内的位号，全部待核实
ite = 0                          # TODO(待核实)
err = 0                          # TODO(待核实)
txfull = 0                       # TODO(待核实)
rxfull = 0                       # TODO(待核实)

[insn]                           # 指令模板，由 impl 编码为机器码
scratch = "r0"                   # 借用的通用寄存器，操作前保存、操作后恢复
read_sysreg = "MRC"              # 32 位；64 位用 MRRC（占两个通用寄存器）
write_sysreg = "MCR"
# DTR 搬运指令的编码：TODO(待核实)

[limits]
poll_timeout_ms = 10
poll_retries = 3

[caps]
borrow_core = true
regfile_needs_halt = true
sysreg_via = "inject"
```

### 8.7 M 核核型号文件（继承与增量）

> 样例：地址取自 Arm 架构手册 / CMSIS 核头文件的约定，实际文件由 `tools/cmsis2regdefs` 生成并经复位值自检复核。省略 `reset` 的寄存器表示复位值因实现而异，不参与自检。样例为简洁省略了 `source` 和 `confidence`，生成的文件必须带上（GEN-02）。

```toml
# regdefs/armv7m-common.toml
name = "armv7m-common"
debug_arch = "armv7m"

[features]
mpu = { present_if = { reg = "MPU_TYPE", field = "DREGION", op = "ne", value = 0 } }

[[reg]]
name = "CPUID"
desc = "CPUID Base Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000ED00 }
avail = { need_halt = false }
read_action = false
fields = [
  { name = "REVISION",     bit = 0,  width = 4,  access = "ro" },
  { name = "PARTNO",       bit = 4,  width = 12, access = "ro" },
  { name = "ARCHITECTURE", bit = 16, width = 4,  access = "ro" },
  { name = "VARIANT",      bit = 20, width = 4,  access = "ro" },
  { name = "IMPLEMENTER",  bit = 24, width = 8,  access = "ro" },
]

[[reg]]
name = "CFSR"
desc = "Configurable Fault Status Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000ED28 }
avail = { need_halt = false }
reset = 0x00000000
fields = [
  { name = "MMFSR", bit = 0,  width = 8,  access = "w1c" },
  { name = "BFSR",  bit = 8,  width = 8,  access = "w1c" },
  { name = "UFSR",  bit = 16, width = 16, access = "w1c" },
]

[[reg]]
name = "SYST_CSR"
desc = "SysTick Control and Status Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000E010 }
avail = { need_halt = false }
read_action = true               # 读会清 COUNTFLAG：不进轮询，只手动读一次
fields = [
  { name = "ENABLE",    bit = 0,  width = 1, access = "rw" },
  { name = "TICKINT",   bit = 1,  width = 1, access = "rw" },
  { name = "CLKSOURCE", bit = 2,  width = 1, access = "rw" },
  { name = "COUNTFLAG", bit = 16, width = 1, access = "rc" },
]

[[reg]]
name = "DHCSR"
desc = "Debug Halting Control and Status Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000EDF0 }
avail = { need_halt = false }
read_action = true               # 读清 S_RESET_ST / S_RETIRE_ST【待核实】；状态检测改用 curstate
write_note = "写入须带 DBGKEY（高 16 位），由后端处理，UI 不暴露"
fields = [
  { name = "C_DEBUGEN",   bit = 0,  width = 1, access = "rw" },
  { name = "C_HALT",      bit = 1,  width = 1, access = "rw" },
  { name = "C_STEP",      bit = 2,  width = 1, access = "rw" },
  { name = "S_REGRDY",    bit = 16, width = 1, access = "ro" },
  { name = "S_HALT",      bit = 17, width = 1, access = "ro" },
  { name = "S_SLEEP",     bit = 18, width = 1, access = "ro" },
  { name = "S_LOCKUP",    bit = 19, width = 1, access = "ro" },
  { name = "S_RETIRE_ST", bit = 24, width = 1, access = "rc" },
  { name = "S_RESET_ST",  bit = 25, width = 1, access = "rc" },
]

[[reg]]
name = "DWT_CYCCNT"
desc = "Cycle Count Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE0001004 }
avail = { need_halt = false, need_enable = ["DEMCR.TRCENA", "DWT_CTRL.CYCCNTENA"] }

[[reg]]
name = "NVIC_ISER"
desc = "Interrupt Set-Enable Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000E100 }
instances = { count_from = "ICTR.INTLINESNUM", count_expr = "n + 1", stride = 4 }   # ISER0..ISERn
avail = { need_halt = false }
fields = [ { name = "SETENA", bit = 0, width = 32, access = "w1s" } ]   # 读回为当前使能状态，写 1 置位

[[reg]]
name = "MPU_CTRL"
desc = "MPU Control Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000ED94 }
avail = { need_halt = false, feature_gate = "mpu" }
```

```toml
# regdefs/cortex-m4.toml —— 只写增量
name = "cortex-m4"
extends = ["armv7m-common"]
partno_expect = 0xC24             # 与 CPUID.PARTNO 复核，取值落地前对照 TRM
optional_features = ["fpu"]

[features]
fpu = { present_if = { reg = "MVFR0", field = "SinglePrecision", op = "ne", value = 0 } }   # 【待核实】

[[reg]]
name = "MVFR0"
desc = "Media and VFP Feature Register 0"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000EF40 }
avail = { need_halt = false }    # 本身是探测用寄存器，不加 feature_gate

[[reg]]
name = "FPCCR"
desc = "Floating-point Context Control Register"
width = 32
scope = "core"
group = "system"
access = { type = "mmio", base = "core_private", off = 0xE000EF34 }
avail = { need_halt = false, feature_gate = "fpu" }
```

`cortex-m7.toml` 同样继承 `armv7m-common`，`partno_expect` 另填，再加 cache 与 TCM 的增量寄存器（MREG-09）。

---

## 9. 开发里程碑

| 阶段 | 内容 | 对应需求 | 阶段验收 |
|---|---|---|---|
| **M0 骨架** | 工程、事件循环、终端恢复、panic hook、**theme 基础**、mock Tcl 服务 | UI-01、NFR-01/07/11 | 空界面空闲 CPU≈0；mock 下可跑 |
| **M1 连接** | OpenOCD 启动/attach、Tcl 客户端与命令队列、日志页、状态栏、Job Object | CONN-01~04、CONN-08、CTRL-01/02 | 连上 F4 板子，halt/run/reset 正常 |
| **M2 M 核看板** | hex 视图、Watch 调度、`CoreDebug`(M)、内置 `armv7m-common` 描述（含继承与 ID 寄存器驱动）、故障分析 | MEM-01/02、WATCH-01~04、CORE-01/02、REG-01~04、MREG-01~07/10 | Watch 全关零读；HardFault 可定位；换 F1/F4/F7 板子自动适配 MPU/NVIC 数量 |
| **M3 描述系统** | SVD 按需加载、chip pack、三层配置解析与覆盖、描述文件 schema 与自动检查、自定义面板、主题热加载、session | PACK-01/02/03/04/08~11、MREG-08/09/11、GEN-02/03/04、UI-03~06、SESS-01 | 新增 F1/F7 只加目录；删除被引用文件时报错可定位到引用处 |
| **M4 gdb** | MI 接入、断点/单步/栈/局部变量、简易源码视图、控制权规则 | GDB-01~05、CTRL-03/04 | gdb 在线时 TUI 不抢控制权 |
| **M5 多核** | 枚举 target、状态栏、全停全跑、归属 key、AP 诊断页、每核 ELF/SVD | MULTI-01~10 | H7 双核可同时看板 |
| **M6 R 核** | `ArmV8R` 后端、核描述生成工具、复位值自检、pre_attach | CORE-04~07、PACK-05/12/13、REG-07/08、GEN-01/05~08/10/11 | R52 复位值自检通过率达标 |
| **M7 观测** | RTT、sparkline、栈水位、跨核对比 | OBS-*、WATCH-06、REG-09 | — |
| **M8 打磨** | 错误提示、帮助页、重连、烧录、文档 | CONN-05~07、CTRL-05、GEN-09 | — |

**原则**：先做 M 核，不依赖借核即可验证整套 UI、Watch、`RegDef`、多核与调度；`CoreDebug` 接口在 M 上稳定后再写 R 后端。第二个 debug 架构的抽象，等有第二个实例时再提炼。

---

## 10. 测试策略

### 10.1 无硬件测试（CI）
- **mock Tcl 服务器**：内存模型 + 可注入故障（超时、断线、保护区读失败、IDR=0），验证队列优先级、Watch 合并、重连、自动关闭失败项。
- **mock 调试 APB**：验证借核状态机（保存/恢复 R0、错误位处理）。
- **配置解析**：golden test（toml、SVD 子集、主题）。
- **零开销断言**：Watch 全关时，mock 服务器收到的读命令数为 0。

### 10.2 硬件测试矩阵
| 目标 | 探针 | 重点 |
|---|---|---|
| STM32F103、F407、F746 | CMSIS-DAP、J-Link | 基本控制、Watch、故障分析；F7 的 D-Cache 提示 |
| STM32H745（双核 M） | 同上 | 多核枚举、全停全跑、每核 ELF |
| S32Z / R52 | 同上 | 通道验证（先读 dbgbase 上固定寄存器）、借核、复位值自检 |

### 10.3 复位值自检
`reset halt` 后批量读全部寄存器，与描述文件的复位值逐个比对，输出可信度清单：
- 能发现：编码抄错、PDF 解析错误、核未实现该寄存器。
- ID 寄存器（MIDR、MPIDR、MPUIR、MVFR 等）用于判断可选特性是否配置。
- 复位值因实现而异的寄存器（如部分 SysTick、ID 类寄存器）在描述文件里不填 `reset`，不参与自检；ID 类寄存器改用字段级期望值（如 `partno_expect`）校验。
- 读副作用寄存器（DHCSR、SysTick CSR 等）不参与批量自检，需要时手动单次读。
- 自检结果可导出为文件（GEN-11），回传给生成流程定位错误条目；通过的条目可升级为 `verified`。
- per-core 的 ID 类寄存器各核应不同、chip 级寄存器各核视角应一致，矛盾即 scope 标错，在诊断页告警。**值相同不能证明 scope，以文档为准。**

### 10.4 描述文件生成流程（TRM/UM → 配置文件，离线，不进 TUI）

分工：**生成方（脚本或 AI）负责把手册转成结构化文件；正确性靠自动检查、人工复核和板上自检来保证。** 生成方碰不到你的硬件，板上验证只能由你来做。流程不限定实现方式，但产出必须符合同一 schema（GEN-02、GEN-04）。

#### 10.4.1 输入与产出对应

| 输入 | 产出 | 能提取的内容 | 不能提取，需另找来源 |
|---|---|---|---|
| 核 TRM（如 Cortex-R52、Cortex-M） | `regdefs/<core>.toml` | 每个寄存器：名称、编码（MRC/MCR 五元组或内存映射地址）、位宽、位域、复位值、访问权限与最低 EL、读副作用、可用性条件、来源页码 | 手册没写清的字段、编码、复位值：标 `fields_missing` / `TODO(待核实)`，不凭记忆补 |
| 核 TRM + Arm ARM（External Debug 章节） | `debug_arch/<arch>.toml` | EDSCR、EDITR、DTR 等偏移与位号、指令模板、OS lock 相关寄存器 | 始终 `verified = false`，板上自检通过后由人改为 true |
| 芯片 UM | `packs/<chip>/chip.toml` 草稿 | Flash/SRAM/TCM 的基址与大小；核数、核型号、cluster 划分；外设基址 | AP 编号、dbgbase、OpenOCD target cfg：靠 ROM table、`dap info`、厂商文档；只列出手册里能找到的线索 |
| 芯片 UM（上电、调试章节） | `pre_attach` 步骤草稿 | 核上电、解除 OS lock、调试授权的步骤（手册有描述时） | 需对照实际板子验证 |
| 厂商 SVD | 外设 `RegDef` | 直接加载（优先） | 无 SVD 时才从 UM 的 PDF 提取，整体标 `low` |
| CMSIS 核头文件 | M 核 `regdefs` | `tools/cmsis2regdefs`（见 MREG-10）：保留 Apache-2.0 声明，组合寄存器（如 CFSR 的 MMFSR/BFSR/UFSR）按位域拆分，v8-M 的安全/非安全别名区单独处理 | — |

#### 10.4.2 流程

1. **确定范围**：用户提供 PDF，指定产出类型和章节范围；记录文档名、编号、版本，以及芯片 revision（手册版本和芯片实际 revision 可能不一致，见 10.4.6）。
2. **判断 PDF 类型**：文字版直接抽取；扫描版先 OCR，整体降级为 `low`，复核比例提高（GEN-09）。
3. **样例先行**：先生成 3 到 5 个寄存器，覆盖一个组合寄存器、一个带读副作用的、一个带 feature gate 的。用户确认格式、命名、scope 和分组后再批量（GEN-08）。
4. **分批生成**：按章节分批，每批产出 toml 片段加本批复核清单，不一次处理整本手册。
5. **自动检查**：跑 `tools/checkregs`（10.4.3），不通过的条目回到第 4 步。
6. **人工复核**：重点看复核清单里的条目：缺失字段、跨页表格、位域排版复杂的页面、编码冲突、手册前后矛盾（`conflict`）。
7. **板上自检**：`reset halt` 后做复位值自检（10.3）；失败项回到第 6 步，通过的条目可升级为 `verified`。需要手动读的读副作用寄存器单独验证。
8. **入库**：生成文件、复核清单、自检报告和手册版本信息一起提交（10.4.6）。

#### 10.4.3 自动检查（GEN-04）

- **编码唯一**：同一 `(cp, opc1, crn, crm, opc2)` 只对应一个寄存器；按 EL 或 security state 分身的，必须显式标注。
- **位域**：不重叠、不超出位宽；报告覆盖率，未定义位标 reserved 或 RES0/RES1。
- **复位值**：与位域定义一致（如复位值落在某字段的非法取值上要告警）。
- **名称唯一**：同一文件内不重名；继承场景下重名必须 `override = true`（MREG-02）。
- **scope 判定**：来自核 TRM 的默认 `core`，来自芯片 UM 的默认 `chip`；手册写明"每核一份"或"cluster 共享"的按文字覆盖；拿不准的标 `unknown`，不猜（见 5.3）。
- **来源必填**：缺 `source` 报错（GEN-02）。

#### 10.4.4 来源与可信度（GEN-02、GEN-05）

每条记录有 `source = { section, page }`，文件头有 `[meta]`（文档名、编号、版本、生成方式与时间）。`confidence` 用一条梯子表示：

| 级别 | 含义 |
|---|---|
| `low` | 来自 OCR、复杂排版，或手册内部矛盾 |
| `medium` | 需要推断（跨页表格合并、由文字描述转成字段） |
| `high` | 结构化表格直接抽取，自动检查通过，编码和复位值已与原文回读核对 |
| `verified` | 板上复位值自检通过；记录日期、板子、固件版本 |

生成时自评 `low/medium/high`，只有自检通过才能升到 `verified`。TUI 诊断页汇总各级别数量，`low/medium` 的条目在寄存器树里显示标记。

#### 10.4.5 AI 处理手册的约定

- **不凭记忆补全**：编码、位域、复位值只来自所给手册；手册没有的标 `TODO(待核实)`。
- **回读校验**：表格抽取后，对编码和复位值等关键数值回读原文再核对一遍。
- **矛盾不裁决**：正文与表格不一致时，两者都记录并标 `conflict`，交给人决定。
- **分批可追溯**：每批列出章节范围和页码；不合并推断；不确定的 scope 标 `unknown`。
- **先样例后批量**：未经用户确认格式，不得批量生成。
- 提示词模板见 13.4。

#### 10.4.6 产物与版本管理

- 一起提交：生成文件、复核清单、自检报告、手册文档名与版本（含芯片 revision）。
- 手册升级时重跑，输出 diff 报告（新增、删除、变更的寄存器与字段），人工确认后合入。
- 板上自检发现与手册不一致（errata、不同 revision）时，**记录差异，不静默修改**。

---

## 11. 风险与待验证项

| # | 项目 | 验证方法 | 影响 |
|---|---|---|---|
| 1 | OpenOCD 对 ARMv8-R（R52）的支持程度，`arm mrc` 是否可用 | 手动敲一次读 SCTLR | 决定 CORE-07 是否需要 |
| 2 | R52 核 `dbgbase` 能否经 `mdw` 读到，是否能从 ROM table 取得 | `reset halt` 后读 EDSCR、外设 ID 寄存器 | **借核方案的前提，先验证再写状态机** |
| 3 | S32Z 的核上电、OS lock、调试授权序列 | 查参考手册，手动执行 | PACK-05 |
| 4 | OpenOCD 状态变化推送方式（`tcl_notifications` 等）随版本不同 | 实测 | 状态检测是订阅还是轮询 |
| 5 | `dap info` / `apid` / `baseaddr` 输出格式 | 实测多个版本 | MULTI-10 |
| 6 | 单核 resume 无按 target 命令，需临时 `targets X; resume` 并切回 | 实测对 gdb 的干扰 | CTRL-01 |
| 7 | OpenOCD 的 `rtt` 命令集与版本 | 实测 | OBS-01 |
| 8 | gdb stub 实际提供的 target description 内容 | `maint print target-description` | GDB-06 |
| 9 | J-Link GDB Server 对 R52 是否提供更完整的寄存器描述 | 查版本与器件列表 | 备选方案 |
| 10 | Arm 机器可读寄存器数据是否覆盖 v8-R AArch32 | 查官方发布内容 | 核描述的数据源，不覆盖则以 TRM 为准 |
| 11 | 借核与 OpenOCD 抢占同一调试单元、OpenOCD 寄存器缓存一致性 | 在板子上用日志迭代 | CORE-05 |
| 12 | 访问未开时钟的外设可能挂总线、产生 AP sticky error | 实测并实现清除 | WATCH-04 |
| 13 | 多核 H7 / M4 需要 boot 选项，才能 examine | 实测 | MULTI-06 |
| 14 | debug_arch 参数文件能否覆盖不同 v8-R 实现间的差异（偏移、位定义、OS lock 序列）；覆盖不了时用 `quirks`，不复制 impl | 对照多款芯片的 TRM 与板上实测 | PACK-07、PACK-11 |
| 15 | M 核寄存器地址架构固定，但数量、有效位随配置变化；NVIC 优先级位数无法由 ID 寄存器读出 | 对照 Arm 架构手册与各核 TRM，多款芯片实测 | MREG-04、MREG-11 |
| 16 | 读 DHCSR 清除 S_RESET_ST/S_RETIRE_ST 的行为，及其与 OpenOCD 复位检测的冲突；`curstate` 是否足够及时 | 查架构手册；实测 TUI 读 DHCSR 前后 OpenOCD 的行为 | MREG-05、CORE-01 |
| 17 | CMSIS 头文件转换的正确性（位域宏命名、组合寄存器拆分）；v8-M 的安全/非安全别名 | 脚本生成后用复位值自检并人工抽查 | MREG-10 |
| 18 | 手册转结构化文件的抽取错误：跨页表格、位域排版复杂、扫描件 OCR | 自动检查、复核清单、板上复位值自检 | GEN-03~05、GEN-09 |
| 19 | 手册版本与芯片实际 revision 不一致（errata、不同 revision） | 文件头记录两者；自检发现差异时记录，不静默修改 | GEN-02、GEN-08 |

---

## 12. 关键参考（Cortex-M，M 核通用）

| 寄存器 | 地址 | 说明 |
|---|---|---|
| DHCSR | 0xE000EDF0 | S_HALT = bit 17；读会清 S_RESET_ST/S_RETIRE_ST【待核实】，标 `read_action` |
| DEMCR | 0xE000EDFC | TRCENA = bit 24 |
| CFSR / HFSR | 0xE000ED28 / 0xE000ED2C | 故障状态 |
| MMFAR / BFAR | 0xE000ED34 / 0xE000ED38 | 故障地址 |
| DWT_CTRL / CYCCNT | 0xE0001000 / 0xE0001004 | 需先使能 |
| CPUID | 0xE000ED00 | PARTNO 用于核型号复核（M3 0xC23、M4 0xC24、M7 0xC27，落地前对照 TRM） |
| ICTR | 0xE000E004 | INTLINESNUM：中断线数上限（以 32 为单位） |
| SysTick CSR | 0xE000E010 | COUNTFLAG = bit 16，读清，标 `read_action` |
| MPU_TYPE | 0xE000ED90 | DREGION 为 region 数，0 表示无 MPU |
| FP_CTRL | 0xE0002000 | FPB 比较器数 |
| NVIC ISER0 | 0xE000E100 | 写 1 置位，读回为当前使能状态 |
| DBGMCU_CR | 0xE0042004 | F1/F4/F7 相同；冻结位设置，防止 Sleep/Stop 时掉线（官方 target cfg 通常已处理） |

特殊 RAM：F4 的 CCM 在 0x10000000，F7 的 DTCM 在 0x20000000，**不得硬编码**，从 chip 文件的 `mem` 表读取。

---

## 13. 给 AI / 人工开发者的约定

### 13.1 工作方式
- 一次任务只实现**一个需求 ID**（或一个里程碑内的一组紧密相关 ID），完成后给出：改动文件、如何验证、遗留的【待验证】项。
- 先写 trait、mock 和测试，再写真实后端。
- 实现前阅读本文档第 2、3、5 章。
- 处理 TRM/UM 手册、生成描述文件时，遵守 10.4.5。

### 13.2 禁止项
1. **不得编造**寄存器偏移、位定义、sysreg 编码、复位值、AP 编号。不确定就写 `TODO(待核实)` 并在 PR 说明中列出。
2. 不得硬编码地址、核数、AP 编号、芯片特有逻辑；一律读配置。
3. 不得在没有启用项时轮询，不得使用固定 tick 空转。
4. 不得在 gdb 在线时通过 Tcl 发 halt/resume/step。
5. 不得依赖 `targets <name>` 切换全局 target。
6. 不得用地址单独作为 Watch/寄存器的 key。
7. 不得把读失败显示成 0；不得对 `read_action` 寄存器自动轮询。
8. 不得为单个寄存器或单颗芯片写专用代码分支（用配置或 `quirks`）。
9. 不得默认引入 tokio、完整 DWARF 解析、脚本引擎，除非先在文档中记录理由与体积/内存影响。
10. 不得省略 panic 时的终端恢复。
11. 不得在 chip 文件里覆盖寄存器定义；不得把 debug_arch 文件的 `verified` 改为 `true`，除非有板上复位值自检通过的记录。
12. 不得用轮询 DHCSR 的方式判断 M 核状态（读会清粘滞位），优先用 `curstate`。
13. 不得假定 NVIC 优先级位数可由 ID 寄存器读出；不得按芯片型号硬编码中断数、MPU region 数等配置值。
14. 处理手册时不得凭记忆补全编码、位域、复位值；手册矛盾时不得自行裁决（标 `conflict`）；不得跳过样例确认直接批量生成。

### 13.3 完成定义（DoD）
- 对应验收标准全部满足。
- mock 测试通过；涉及硬件的部分有手动验证记录。
- 空闲 CPU 与内存未回归（记录前后数值）。
- 新增配置项有示例和校验错误提示。
- 【待验证】项已更新到第 11 章（结果或新增风险）。

### 13.4 任务提示词模板
```
请实现需求 <ID>：<需求描述>。
约束：遵守 13.2 的禁止项；读取第 5 章的数据模型。
不确定的寄存器信息请标 TODO(待核实)，不要编造。
输出：改动文件、mock 测试、手动验证步骤、遗留待验证项。
```

处理手册（生成描述文件）时：
```
请根据附件 <文档名/版本> 的 <章节或页码范围> 生成 <regdefs | debug_arch | chip> 文件。
先输出 3 到 5 个样例寄存器供我确认格式，确认后再按章节分批生成。
每条记录带 source（章节、页码）和 confidence；缺失内容标 fields_missing 或 TODO(待核实)，
手册前后矛盾标 conflict，不要凭记忆补全，不要自行裁决。
输出：toml 片段、本批复核清单、自动检查结果。
```

---

## 附录 A

### A.1 项目目录结构
```
tui-debugger/
  Cargo.toml
  src/
    main.rs                事件循环、终端初始化与恢复
    app/                   应用状态、命令、脏标记
    ui/                    面板组件、主题、布局、键位
    link/
      tcl.rs               Tcl 客户端与命令队列
      gdb_mi.rs            MI 子进程与解析
      proc.rs              OpenOCD/gdb 进程、Job Object
    core/
      mod.rs               CoreDebug trait
      cortex_m.rs          M 核后端
      armv8r.rs            R 核后端（借核）
    model/
      regdef.rs            RegDef/Scope/Access/Avail
      watch.rs             Watch 调度
      svd.rs               SVD 加载（按需）
      elf.rs               符号表
    config/                chip/probe/theme/session、核型号、debug_arch 的加载、解析与校验
  packs/
    stm32f407/  chip.toml  svd/  views/  init.tcl
    s32z/       chip.toml  svd/  views/  init.tcl  regdefs/  debug_arch/   # 后两者可选，优先于全局同名文件
  probes/       jlink.toml  cmsis-dap.toml
  themes/       default.toml  mono.toml
  regdefs/      armv7m-common.toml  cortex-m3.toml  cortex-m4.toml  cortex-m7.toml   # M 核文件内置随程序打包
                cortex-r52.toml
  debug_arch/   armv7m.toml  armv8r_a32.toml
  tools/        trm2regdefs/（离线生成与复核）
                cmsis2regdefs/（从 CMSIS 核头文件生成 M 核描述）
                um2chip/（UM → chip.toml 草稿与待填项清单）
                checkregs/（描述文件自动检查，GEN-04）
  tests/        mock_tcl/  mock_dap/  golden/
  docs/         本文档
```

### A.2 OpenOCD 启动示例
```
openocd -f interface/jlink.cfg \
        -c "transport select swd" \
        -c "adapter speed 4000" \
        -f target/stm32f4x.cfg
```
默认端口：gdb 3333（多核依次递增）、telnet 4444、Tcl 6666。绑定本机地址的具体命令请按所用版本确认。

### A.3 OpenOCD 多 target 写法（参考）
```tcl
dap create $CHIP.dap -chain-position $CHIP.cpu
target create $CHIP.cpu0 cortex_m -dap $CHIP.dap -ap-num 0
target create $CHIP.bus  mem_ap   -dap $CHIP.dap -ap-num 1
```
TUI 不自行创建 target：沿用官方 target cfg；需要附加 `mem_ap` 时生成 `user_ap.cfg` 并重启 OpenOCD。attach 模式下只能使用对方已建好的 target。

### A.4 常用 Tcl 命令（均需按版本实测）
```
targets                          # 枚举 target
<t> curstate                     # 状态（OpenOCD 缓存，不占链路）
<t> mdw <addr> [count]           # 读内存
read_memory <addr> <width> <count>
<t> arp_halt / arp_examine
<dap> info <ap>                  # AP 类型与 ROM table
<dap> apid <ap> / baseaddr <ap>
```

### A.5 术语
- **DP / AP**：Debug Port / Access Port。**MEM-AP**：可读写内存的 AP。
- **dbgbase**：核调试寄存器所在 APB 的基址（来自 ROM table 或 profile）。
- **借核执行**：核 halt 后，经 EDITR 注入指令（如 MRC）并通过 DTR 取回结果，用于读取没有地址的 sysreg。
- **per-core / per-chip**：寄存器归属范围，决定实例化份数和挂载位置。
- **banked**：同地址、按访问者看到各自副本的寄存器。
- **torn read**：非原子读取到前后不一致的值。
