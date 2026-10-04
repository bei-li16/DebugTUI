# 寄存器状态计数与读取取消（开发分支）

对应 REG-205、REG-206、REG-211，软件回归关联 REG-T04/T05/T06/T07/T08/T11。尚未进入 v0.9.3 release。

## 计数与界面

System Regs 的 Total 是目录全部定义；Shown 是目标／全部定义、分类和搜索筛选后已展开的寄存器行，包括滚动范围外的行，排除组标题和字段；Valid 是这些行中当前暂停、会话、停止代次、实际 owner 和适用帧均匹配且有原始值的样本。继续运行或上下文过期后旧值可保留，但不计 Valid。

点击 Status、按 `t` 或执行 `:register-status` 查看可读定义数量及八种互斥状态：Not read、Valid、Not implemented、Unavailable、Reader unsupported、Error、Write only、Stale。Readable definitions 按访问属性和已知实现判断，并不等于读取成功。方向键、PageUp/PageDown、Home/End、滚轮可浏览；Esc、Enter 或 Close 关闭。查看统计不会访问目标。

状态行与计数使用同一规则。失败可显示上次精确值，但附当前状态，不冒充成功。硬件缺失与 reader 缺口分别显示；未知 Unsupported 原因保留为 Unavailable。配置／当前探测确认的缺失项在 Target 视图隐藏；单项实际采样确认缺失也隐藏。All 视图保留定义与原因，但不自动试读。单项缺失证据随核心／停止上下文过期。

## 取消语义

点击 Cancel read、执行 `:register-cancel`，或在 System Regs 非搜索模式按 Esc；搜索模式的 Esc 仍只取消搜索。MPU 总览关闭或切 bank 会取消未完成的寄存器读取。

取消范围包含按需／手工读取、Probe、selector bank 和直接 MPU 总览；在途限制持续到 worker 回复。当前 MI 请求或原子 Tcl／后端事务完整结束，选择器、scratch、物理状态及 target 的恢复检查继续执行；随后停止后续项目，丢弃该批次全部新样本。保留已有值及仍属当前上下文的能力证据；不会自动重试该批次，可明确手工重读。取消不退出会话、不继续核心、不广播至 Scope All 的其他核心。

恢复结果未知仍隔离共享服务并进入 FAULT；取消不能覆盖这类错误。请求已完成时取消不能撤回已完成的读取。Rust 客户端可保留 `Request` clone 后调用 `cancel_read()`；标记只在进程内共享且不由 JSON 接收。Headless JSON 当前没有远程取消 API。

## EL1／Guest 的架构核对

用户提供的 `Armv8-R AArch32.pdf` 是 DEN0130 简介；同目录完整架构补充为 `ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf`，DDI 0568A.c。结合 Cortex-R52 TRM 100026_0104_01_en，本轮核对：

- E2.1.4（E2-88）：HCPTR 仅 EL2 可访问，不能先在 EL1 读取它来证明 VFP 无陷阱；EL1 访问受 HSTR.T1 影响，可能异常或陷入 Hyp。
- H1.2（H1-247–248）：Advanced SIMD／FP 检查仍涉及 CPACR、FPEXC、HCPTR；调试态没有通用绕过。FPSID 与 MVFR 的 ID 访问还须分别核对 HCR 对应陷阱位。
- F1.3.4（F1-189）：EDSCR.HDD 可以禁止 DCPS2，并把部分 EL2 陷阱变为 Undefined，不能当作可读取证明。
- H1.3（H1-295）：DCPS2 切换至 Hyp，ELR_hyp、HSR、SPSR、DLR、DSPSR 等可能成为架构 UNKNOWN；简单包一层 DCPS/DRPS 无法证明所有原状态可恢复。

当前 Hyp 只读 VFP 实现保留；EL1／Guest／User 的合法读取仍未完成。后续需验证不破坏银行状态的专用后端或受控目标协作方案，不能把权限拒绝、无模式切换的声明或现有软件测试算作完整支持。

## 验证与限制

`src/ui/registers/status/tests.rs` 覆盖分类计数、字段排除、筛选／折叠、owner／session／停止／帧、宽窄中文、键鼠、实际缺失过滤、旧值与迟到结果。`tests/register_cancel.rs` 用实际 worker／MI 验证排队前取消、在途及末项取消、批次丢弃、旧 Probe 保留、无全局退出和新控制请求。`tests/selector_access/cancel_cases.rs` 执行真实 Tcl 控制流，覆盖 selector 恢复、恢复失败优先、直接 MPU、VFP pair 和 Scope All 隔离。

延后人工用例见 [状态与取消验收](../tests/cases/register-status-cancel.md)。本轮未执行上板或 PowerShell／VS Code 终端实际视觉验收。完整 TODO 仍待完成；暂不安装、推送或发布。
