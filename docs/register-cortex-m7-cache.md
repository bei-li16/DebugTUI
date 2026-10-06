# Cortex-M7 cache/TCM 只读访问

本批对应固定 Goal 的 B08，复用 M7 增量目录、CorePrivate、worker、服务 lease、MI/Tcl 及现有 MPU 总览控制器。没有增加 CPU 指令注入器、writer、cache maintenance 或自动使能功能。没有执行上板测试。

M7 目录提供 CLIDR、CTR、CCSIDR、CSSELR、CCR、CACR、ITCMCR、DTCMCR、AHBPCR、AHBSCR；TCM/configuration 复用普通 CorePrivate 读取及字段树。维护命令 ICIALLU/ICIMVAU/DCIMVAC/DCISW/DCCMVAU/DCCMVAC/DCCSW/DCCIMVAC/DCCISW 保留为 WO 定义，显式 manual 请求也不能发送读取或执行命令。

## cache ID 事务

先在当前物理 frame 0 暂停并执行 Probe。CPUID 必须匹配 Cortex-M7；CLIDR、CTR 必须来自已完成、同核、同上下文的实际观测，配置声明不能代替证明。CLIDR 的 ICache/DCache 位表示实现，独立于 CCR 的使能位。缺失、非法或较新的失败观测撤销有效事实。

`:cache` 或 System Regs 的 **M7 cache** 打开 I/D cache 总览。打开、滚动、字段显示及关闭零目标 I/O；`p` 显式 Probe，`r` 显式 Read。需要该核明确的独立 Tcl/AP target；仅有 GDB memory 路线时返回不支持，不执行未标明 bank 的 CCSIDR 读取。

一次 Read 在同一服务锁和单个显式 target Tcl 请求内：检查停止状态及 CPUID/CLIDR/CTR，保存 CSSELR，只选择 CLIDR 已实现的 data(0)/instruction(1) bank，回读选择值，读取各 CCSIDR，恢复并回读原 CSSELR，复核身份/配置/停止状态，最后检查 MI 线程和 frame。写操作仅为事务所需的 CSSELR；没有全局 target 切换或运行控制。选择写入即使已生效后报错，也执行恢复。

普通 `scb.ccsidr` 及位于同一 CCSIDR 地址的目录 reader 复用该事务，返回原 CSSELR 对应的 cache 样本。当前选择未实现的 cache 时明确 HardwareNotImplemented，总览仍能查看已实现的另一 bank。CLIDR 证明无 cache 时总览为空，不访问 CSSELR/CCSIDR，也不制造零 selector 样本。

完整 bank 样本位于 `Snapshot.register_cache`，与普通目录样本分开保存，包含 owner、context、原/恢复 selector、各 cache 类型及实际 Tcl 请求的来源和时间区间。错误、取消或物理上下文变化不发布部分成功值；旧值保留原时间/来源并 stale。恢复失败、断线或损坏的响应导致 FAULT/服务隔离；后续读取不自动重试。

总览仅为手册明确支持的编码显示 4～64 KiB 容量；未知 CCSIDR 编码保留原值，容量显示 unknown encoding。stale 值不继续派生字段或容量。运行、frame/session/generation 变化及最新 CLIDR/CTR 失效撤销 bank 有效性；多核缓存与路由使用实际 owner/context，各核独立保存 selector。

Headless 请求（context 从当前 `registers_list` 获取）：

```json
{"method":"registers_cache","params":{"context":{"session":1,"generation":2,"core":"core0","frame":0},"read":true}}
```

省略 `read` 或使用 `read:false` 只取缓存，零目标 I/O。示例 context 不能照抄到其他会话。

## 来源与验证边界

地址/TCM 字段来自固定版本 CMSIS `core_cm7.h`，来源与 Apache-2.0 声明保留在 `third_party/cmsis-core`。CLIDR 的 M7 I/D 位由 CMSIS 缺失项补齐；cache ID/selector 字段、有效编码和容量依据 [Cortex-M7 Devices Generic User Guide DUI 0646B](https://documentation-service.arm.com/static/61efd6602dd99944d051417b) §4.5.1～4.5.4，Tables 4-40～4-44，物理 PDF 第 269～271 页（打印页 4-38～4-40）。没有填写实现相关 reset，也没有升级硬件 verified。

`scripts/test-cmsis-registers.py` 与六目录离线再生成核对来源、CLIDR 字段和维护命令隔离。模型/UI 专项验证身份、编码、解析、容量和零 I/O；`tests/selector_access/m_cache_cases.rs` 通过真实生产 worker、MI、TCP 与 Tcl 解释器覆盖正向 bank、普通 CCSIDR 委托、TCM raw、WO、未实现/未知编码、恢复、故障隔离、取消、迟到上下文、双核及实际 EXE 驱动。软件夹具提供独立原始响应，不模拟 ARM 指令执行，不证明实板适配。延后环境 case 及默认四项 SKIPPED 的驱动见 [M7 cache 用例](../tests/cases/register-cortex-m7-cache.md)。

本批不代表 B10 完整异构生命周期、B11 运行态入口或 R52 当前 Debug 权限已完成。核心引用保留：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
