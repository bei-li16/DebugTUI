# Bao 与多 VM 调试验收记录

目标为同一核心在 Bao 与所属 Guest 之间切换时，保留连接并正确解析源码、变量、异常上下文和可恢复栈。符号共存不等于全部目标已完成。

## 当前进度

- 开发分支：`codex/bao-multi-elf`，起点 `bbdb12d`。
- 实板：THA6206 双核，两个核心实读 MIDR 均为 `0x410fd161`；CMSIS-DAP Vllink.Basic2，SWD 5 MHz。
- Bao 仓库已有用户修改，保留原有修改；实际源码为 `BAOHyp/bao-hypervisor`。
- 已实现每核常驻镜像、ELF 可执行段/重定位检查、停止程序 CPSR 与 PC 识别、Setup 镜像编辑、`:images` 和只读 Guest 上下文查看。
- 保存 Guest 的 SP/LR 来自 Bao 自然执行的诊断快照，未使用跳过的 `vcpu->regs.x[13]`。诊断代码仅在 `BAO_DEBUG_GUEST_CONTEXT` 构建选项下启用。
- 物理 GDB 在 Bao 内读取 Guest 栈会返回与 AHB 不一致的数据；当前随包 GDB 不含 Python。实现了本地只读 RSP 解析通道：辅助 GDB 使用真实保存寄存器和受范围约束的 AHB 读取，不连接物理 GDB 端口，不写寄存器、不运行目标函数。
- 同名代码/数据断点、Bao/VM 全局与局部值、Watch 树、运行中镜像标量、独立源码浏览及 HVC 入口/返回边界已有实板证据，详见后续阶段。完整目标仍未完成，取消/暂停、资源耗尽、词法遮蔽和最终统一验收仍需继续。

## 完整验收范围

| 项目 | 当前证据 |
|---|---|
| 多镜像配置、Setup 与工作区入口、配置恢复 | 配置/Setup 单元测试通过；工作区 `:images`、`:guest`；完整恢复回归待完成 |
| A 两核 Bao 启动、B 两侧 Guest 源码 | 两核 init、各自 guest_main 实板通过 |
| C HVC 全链、D 物理/虚拟 IRQ 全链 | HVC 向量、部分 VM_EXIT、保存完成、分派、VM_ENTRY/ERET 通过；IRQ 保存/恢复中间阶段、物理 INTID 30→Bao→Guest irq_handler→SVC 返回通过 |
| E 全局/局部/同名符号、F 挂起 Guest 寄存器与栈 | 两侧 Guest 的 notify→guest_main→guest_reset 恢复通过；VM0 保存的 r0、handled、tick、seq 等真实值通过，payload 正确显示 optimized out；同名全局/符号边界尚未完整覆盖 |
| G 双核上下文隔离、H 断点与跨域单步 | core0 在 VM0、core1 在 Bao 时独立恢复 VM1 通过；启动前 Guest 硬件断点、条件 IRQ 断点、显式 HVC 返回及途中真实用户断点优先均实板通过；取消只删除自有断点的软件管道测试通过，取消实板尚未覆盖 |
| I 至少 20 次往返与资源检查 | 20 次真实 VM0 HVC 往返通过，无残留临时断点；仅首尾检查完整 Guest 栈，后续还需结合最终实现回归 |
| J 失败/歧义/缺失输入、K 恢复及单 ELF 回归 | ELF 解析边界/重叠/重定位、快照 ABI/身份/SP、旧 UI 响应丢弃单元测试通过；同路径 ELF 修改在 MI 求值前拒绝、损坏 ELF 在共享服务前拒绝、部分加载失败回滚两核且 symbols_loaded=false 通过；原有单 ELF/双真实 GDB 多核驱动 13 项、原生 Watch 树通过；全部负例及配置恢复仍待完成 |
| 无隐藏重连/复位/下载/暂停的命令及进程证据 | 实板驱动比较物理 MI 控制日志，20 次往返及 IRQ 过程中未增加控制命令；混合核测试仅有显式准备暂停；辅助 GDB 使用独立 guest-mi 日志 |
| Headless 与实际 TUI 验证、产物摘要、阶段清理 | 当前构建 Headless 实板 42 项、真实 ConPTY TUI 9 项、软件镜像会话 10 项、真实 ARM GDB 文件测试 9 项通过；完整 lib 598 通过、0 失败、3 个原有忽略；7 项集成和 clippy all-targets -D warnings 通过。已保留已测开发 EXE、符号归档和原始证据；target 仍用于剩余验证，完整目标收尾时执行清理 |

## 2026-10-27 原始证据

均位于仓库 `artifacts/`，硬件证据未去重或删减：

- `bao-images-hardware-1793077409921-f369f91d/report.json`：12/12，使用 EXE SHA256 `adc3ae96a3831f9946601fca6053f0b49e68f1b397542af18b9d6797afc7eab3`。
- 同目录 `twenty-real-hvc-round-trips.json`、`guest-round-19.json`、`physical-irq-saved-guest.json`、`mixed-cores-saved-vm1.json`：真实循环、局部变量、IRQ 和另一核心的保存栈。
- `bao-images-terminal-20261027-130454-8ff0cece/`：6/6，真实板卡上的 ConPTY 输入/输出；保留 `.screen.txt` 和原始 `terminal.vt.txt`。最小终端 45×12，恢复 160×45，Esc 返回同一物理 Bao 帧。
- `multi-images-software-1793077449181-a26e92a3/report.json`：6/6，软件夹具明确标为未使用实板，包含 20 次模拟域切换及 ELF 同路径变化。
- 初次 Guest 断点失败发现 BREAK API 忽略 kind=hardware、软件断点被 Bao RAM 复制覆盖，已修复为多镜像代码断点使用硬件断点；原失败轮次 `bao-images-hardware-1793071028963-5484f82e` 保留。
- 最初 CPSR 查询缺少 thread，随后 thread 加引号又被真实 GDB 拒绝；现使用已校验的裸十进制线程号，实板读到 Bao Hyp / Guest SVC / Guest IRQ 模式。
- IRQ 夹具曾误用参数名 id（实际 int_id），及在内联 timer_reload 处查询不存在的局部 id；已改为实际源码作用域，物理/虚拟 INTID 30 均验证。

以上 EXE 是开发构建，安装的正式版本仍为 1.2.4；后续源码/EXE 变化必须以新报告摘要为准。

### 本阶段最终构建

- EXE：仓库 `bin/debugtui-bao-development.exe`，SHA256 `4162cd0b2a7b4404e3f9a1d46d693b0a92d8e5bdd1219b07b2b5ed0cfb363e98`。这是本阶段上板测试 EXE 的逐字节保留件，未覆盖已安装正式版。
- 实板：`bao-images-hardware-1793078576186-151e8c91/report.json`，13/13。
- TUI：`bao-images-terminal-20261027-132426-babb49a2/report.json`，7/7，包含 Guest 界面按 r 返回 Guest。
- 软件：`multi-images-software-1793078637757-b0906bdf/report.json`，8/8。
- 兼容回归：`multicore-1793078710963/verification.json`，13 项；`watch-tree-1793078726235/verification.json`，原生 GDB Watch 树通过。
- Bao ELF SHA256：`50384620c87fce6ff602a20d4d6786ce377390716396e5abb37d19fbf33d2bbd`；Guest0：`0108a559669944509a45c693cf8e93481dbcd42dee474cd16c3cf970fded222a`；Guest1：`c62c3f0c1660cb96720fe5d8f4fed353a9b759dc5f36106932183a2425a0c309`。
- `bao-multi-elf-development-451f8578/stage-manifest.json` 保存阶段摘要、ELF/EXE 校验值和清理结果；`retained-exe-query-output.jsonl` 证明保留 EXE 可在原位置解析 builtin 工具和实际 Bao 配置，无目标 IO。
- 清理前 target 文件合计 4,104,270,817 字节；保留开发 EXE 173,940,205 字节；净减少 3,930,330,612 字节。源码、用户工程、工具链、正式安装、报告及全部原始实板日志保留。

## 下一阶段必须补齐的目标

1. 同名符号/断点和多镜像运行时 Watch 已在后续阶段实现，仍需将其纳入最终 A–K 统一验收。隔离全局目前只读，混合局部/全局表达式及命名全局条件断点尚有限制。
2. 检查运行中镜像绑定与手动浏览归属的组合；运行值来自显式内存地址，身份来自最后已知上下文，不将其作为实时 Trace。继续覆盖最终缓存、写入预览与异步旧响应负例。
3. Files/符号搜索/反汇编和手动源码浏览的镜像选择；处理各镜像相同远端前缀映射到不同本地根时，GDB 全局 substitute-path 的冲突，避免用当前执行镜像覆盖手动浏览归属。
4. HVC/VM_EXIT/VM_ENTRY/ERET 的入口与中间阶段逐指令验证；实际取消、用户暂停、资源不足及途中异常回归。不能用已有 hvc_handler/返回点验证替代整条链。
5. 缺 DWARF、源码缺失、运行时地址歧义、ABI 不匹配及中途 ELF 替换等完整负例；保存配置重新启动后的多镜像恢复；Bao 诊断开关关闭的构建回归。
6. 对最终代码重新执行实板 A–K、真实 TUI、兼容性检查及清理；目前保持完整目标未完成。

后续每项以实际产物、命令、日志和测试结果更新。本记录不授予任何硬件访问能力，也不替代原始证据。

## 同名符号与运行中 Watch 阶段（2026-10-27）

已实现并实测：

- 断点保存镜像 ID 和原始名称，通过单 ELF 离线 GDB 解析为精确地址后创建物理硬件断点；支持 `@vm0:函数` 和编辑器 Image 选择，镜像不属于当前核时先拒绝。
- Bao、VM0、VM1 刻意包含同名 `debug_domain_marker`、`debug_domain_id`、`debug_domain_counter`，诊断编译开关控制这些测试符号。两侧全局值、真实局部值、Guest RAM 复制前断点、数据断点及保存/重新启动恢复均已验证。
- 全局表达式和 Watch 使用单 ELF 辅助 GDB 解析类型，通过配置的 AHB 获取实板数据。辅助寄存器只提供已观测 PC/CPSR，其他寄存器标记 unavailable；物理局部表达式仍保留真实帧。结构体展开和成员地址解析沿用同一镜像，界面显示归属。
- 新增 `images.memory_channel`（Setup 可编辑）。运行时标量读取从各镜像解析及重定位地址，按核和 ELF 内容版本绑定，不再只读取 `live_watch.elf`；切核、旧代次和 ELF 变更结果被拒绝，显示 last-known，不宣称 Trace。
- 诊断固件增加 `.debug_frame` 异常边界 CFI；Bao GCC 配置启用 DWARF unwind。参考配置和隔离全局 GDB 从匹配 ELF 读取只读代码，减少跨 MPU 域的代码探测。可变全局值由实板通道读取，没有通过修改 HSR、SP 或物理模式修复调试状态。
- UI 镜像版本/归属变化会使旧内存绑定、显示样本和写入预览失效。隔离全局目前拒绝写入；混合局部/全局表达式、命名全局条件断点仍有限制，未作为完整目标完成。

当前开发 EXE：`bin/debugtui-bao-images-development.exe`，SHA256 `ecd0074aab0907975045e3dafbd0b53f5d39b65f444eb500ddd355a7858fb292`。正式安装仍为 1.2.4；开发 EXE 未发布或替换全局安装。

该 EXE 的实板证据：

| 范围 | 报告 |
|---|---|
| 同名函数/全局/局部、Watch 树/类型地址、只读写入拒绝、数据断点、保存恢复 | `bao-image-symbols-hardware-1793099979494-320c7fc3/report.json`：12/12 |
| HVC、保存 Guest、20 次往返、物理/虚拟 IRQ、混合双核 | `bao-images-hardware-1793099997892-2d5211c9/report.json`：13/13 |
| 两核运行中 Bao/VM 标量、切核、ELF 替换拒绝旧绑定 | `bao-images-live-hardware-1793100029671-c532b246/report.json`：5/5 |
| 从保留 EXE 直接启动的真实 TUI、Guest 栈/返回、Watch 镜像标签和值 | `bao-images-terminal-20261027-192450-33e2e1ed/report.json`：8/8，保留原始 VT 与 screen.txt |

恢复诊断构建后的 ELF SHA256：Bao `e3c48aefa6b5c2f304930eba3cece088ca532b7808f174de127cb8c290b52494`，VM0 `a040ce9e630b6db39a7d2f59cb91553cf659eacc79d671ff1ccfecd49f8c8847`，VM1 `cc08f57b53955801b30aa9b1baea0d9caa940f668e674580f8159c2fad242332`。三份符号原件另保存在阶段目录的 `verified-bao-vm-symbols.zip`。

软件及兼容回归：`multi-images-software-1793085903762-44e43333` 8/8；`watch-tree-1793085907687`、`memory-native-1793085912643` 真实本地 GDB 通过；`multicore-1793085919157` 13 项通过。最终 lib 594 通过、0 失败、3 个原有忽略，clippy all-targets `-D warnings` 通过；任务/生命周期/驱动集成串行 6 项通过。Cargo 原始输出在 `bao-images-symbols-b5a38e27/`。

排查记录保留，不将早期失败算作通过：

1. `bao-image-symbols-hardware-1793080536882-f734a2e6`：VM0 未命中预期断点，停在 Bao panic；异常栈边界存在越界探测及调试 abort，诊断 CFI 边界修复后继续进入 VM。
2. `bao-image-symbols-hardware-1793081700700-127ba635`：VM0 全局返回 0；改为单 ELF 解析和明确 AHB 实读，验证真实 ID。后续还修复了 `(unsigned long)&变量` 被只读表达式校验误拦截的问题。
3. `bao-images-terminal-20261027-150552-5507f4a6`：全局值正确，但镜像标签只传到了格式项，实际 Watch 行未绘制；修正后真实 TUI 8/8。
4. 生命周期兼容测试在并行负载下曾发生 15 秒响应等待超时；单独用例重跑与完整串行 6 项通过，保留 `cargo-integration-final.log`、`lifecycle-rerun2.log`、`lifecycle-isolated.log`、`cargo-integration-serial.log`。没有放宽产品或测试超时；并发负载与失败相关，尚不足以认定唯一根因。
5. 关闭诊断开关构建成功，GDB 确认诊断符号消失，随后恢复构建。Bao ELF/BIN/HEX 完全相同；Guest ELF 文件摘要因临时对象名称变化而不同。VM0 原/新 ELF 对比仅 `.strtab` 中的 `ccJMBWxR.o` / `cclzWiuO.o` 不同，所有装载段和 DWARF 相同；两侧恢复后的 ELF 再次通过上表实板验证。保留摘要差异，不将不同 ELF 摘要说成一致。

完整目标继续保持未完成，重点剩余项：

- Files、符号搜索、手动浏览、反汇编和冲突路径映射的问题已在下一阶段修复；以下各阶段的 EXE/ELF 摘要和实测范围分别记录，不能混用旧程序的报告证明新程序。
- HVC/VM_EXIT/VM_ENTRY/ERET 入口及保存/恢复中间状态的逐指令链，真实取消/暂停/资源耗尽和异常优先的完整负例。
- 隔离全局写入、混合表达式及命名全局条件的正确镜像绑定，避免把拒绝不安全操作等同于全部功能已经支持。
- 缺 DWARF/缺源码/地址歧义/ABI 不匹配等完整 J 项，最后按完整 A–K 和当前交付产物统一审计。未验证运行中新增 ELF，不以启动前装载代替该证据。

本阶段已执行 `pwsh -NoProfile -File scripts/cleanup-build.ps1 -Apply`：Cargo 删除 2,974 个文件，target 文件逻辑大小 4,141,761,972 字节（约 3.86 GiB）；D 盘同期可用空间实际增加 3,278,929,920 字节（约 3.05 GiB），target 目录已移除。保留本阶段已测 EXE（176,585,185 字节）、符号归档、源码/工具链/工程配置、正式安装、报告及全部原始实板/失败证据。精确数值及 SHA256 记录在 `bao-images-symbols-b5a38e27/stage-manifest.json`，执行输出为 `cleanup-final.log`。

## 源码归属与异常入口阶段（2026-10-28）

- SourceDocument、Files、Symbols、断点源码及反汇编持有独立镜像归属；传输引用为 `@image:路径`，兼容盘符和两种斜杠。停止快照按新镜像读取源码，手动浏览保留物理帧。标题分别显示 Exec / Browse / Frame，运行时标记 last known。
- 每个镜像使用自己的路径规则和编译目录记录。显式映射的文件缺失时不接受 GDB 的同名文件搜索回退；未知或歧义 PC 不展开物理栈或使用旧局部值，原始寄存器查看仍可用。
- 辅助 GDB 同步独立的架构声明。首次逐指令测试发现未同步 `armv8-r` 时，HVC 被解码成 CMP；修复后真实 HVC/ERET 正确解码。
- HVC 停在 Hyp 向量时，GDB 曾猜测普通调用栈，出现 Debug abort，保存 HSR 从 HVC32 变成 `0x96000004`。给诊断向量表补充 CFI 后，HVC 入口、部分 VM_EXIT、有效快照、分派、VM_ENTRY 和 ERET 的实板测试 5/5 通过，检查期间没有新增 abort。BIN 摘要保持 `5f990994aaf0c96d0cc59801e0afd8d5fabe98e44155aa1f93fae697a462c256`，未重新下载程序；变更仅位于调试信息。

本阶段已有原始证据（各报告自行记录 EXE 摘要，后续新构建不能沿用为自身证明）：

| 范围 | 报告 |
|---|---|
| HVC/IRQ 保存和恢复中间阶段、真实 Guest 栈及 ERET | `bao-images-edges-hardware-1793157036201-390d5f25/report.json`：7/7 |
| 两核、HVC、20 次往返、物理/虚拟 IRQ、混合域；同一组物理 GDB/OpenOCD PID | `bao-images-hardware-1793157056492-da297831/report.json`：13/13；另有 `physical-process-continuity.json` |
| 停住/运行中的文件、同名符号和 ELF 反汇编 | `bao-images-navigation-hardware-1793157134089-82823394/report.json`：5/5 |
| 同名值、Watch 树、数据断点、保存恢复 | `bao-image-symbols-hardware-1793157146498-f7c93e3b/report.json`：12/12 |
| 运行中分核标量与 ELF 更新拒绝旧地址 | `bao-images-live-hardware-1793157169511-69b865a7/report.json`：5/5 |
| 真实 TUI 手动打开 Bao/VM0 源码，物理 VM0 帧不变 | `bao-images-terminal-20261028-111308-f23f8795/report.json`：9/9 |
| ARM GCC/GDB：冲突映射、路径、源码缺失、去除 DWARF、重定位和 HVC 指令 | `image-navigation-1793156909500-c7955422/report.json`：9/9，无实板访问 |
| 生产会话管道：未知/歧义 PC、20 次模拟切换、旧 ELF、非法文件及部分启动回滚 | `multi-images-software-1793156878654-7146bf17/report.json`：10/10，目标响应为软件夹具 |

重建的调试符号为 Bao `e767f4a5536d0ffec2ffe5bc19e1801beeca192adf2346893d75afcd9cec839e`，VM0 `9a0ed10fd53ffe15bed74ad4aaa12d27dedb91066e2f571855ce1033b7ad99e6`，VM1 `83924a5b9528b83b5c4d20b57b4bfc33a402ef17d96c28947a4c4dba2b9987c9`。BIN 一致性及构建输出在 `bao-navigation-development-d2ff5152/bao-vector-cfi-artifacts.json`、`bao-vector-cfi-build.log`。

本阶段已测 EXE 为 `bin/debugtui-bao-navigation-development.exe`，SHA256 `59c22f74653c2c054904f5c147055da449ba6e91cf76f54c511aca5fc8e59c81`。上表报告已逐一核对与此 EXE 摘要一致。`bao-navigation-development-d2ff5152/stage-manifest.json` 保存清单及未完成项；`verified-bao-navigation-symbols.zip` 逐项核验与原 ELF 一致，ZIP SHA256 为 `4273a6045868f0979495e5f709f1c192d5d88af199b930e6e5250d988d93346e`。

真实取消/暂停、断点资源耗尽、全局与局部同名的词法遮蔽和完整本域表达式行为，以及最终完整 A–K 审核仍需继续，不能仅由上表认定目标完成。原有单 ELF 和多核驱动分别见 `watch-tree-1793156962718`、`memory-native-1793156974298`、`multicore-1793156990793`（13 项）。

环境及回归排查：

- Windows 提交余量曾降至约 580 MB，构建中断；经用户授权重启资源管理器主进程后回收约 8.5 GiB 私有提交，未关闭其他用户应用。证据为 `explorer-restart-authorized.json`，中断的构建日志保留。
- 新路径单元测试发现前缀匹配误用于文件名后缀，修复后通过。一次去除 DWARF 的 WSL 第二次启动超时，改为同一夹具构建中完成后，9 项真实 GDB 测试通过。
- 旧构建全量库测试 597 通过，集成中的首次双核 connect 超过 15 秒观察窗口。跟踪显示约 3.8 MB 的文件目录反复解析，首核 GDB 通信本身约 0.65 秒。新增有界、按内容匹配的声明解析缓存，只复用 TOML 解析出的目录声明，文件来源和继承仍逐次处理，不共享寄存器值或访问证明。9 项目录测试、原时限下的生命周期复测和完整 7 项集成通过，未放宽超时。最终库测试 598 通过、0 失败、3 个原有忽略，耗时 133.65 秒；`cargo-lib-cached-final.log`、`cargo-integration-cached-final.log`、`clippy-cached-final.log` 保留原始输出。
- 集成歧义测试起初将正常的 `-break-list` 查询也当成禁止命令；核对日志后改为只允许这一只读查询，仍严格禁止歧义断点插入和控制命令，并单独检查类型求值在 MI 前被拒绝。原失败轮次 `multi-images-software-1793156608707-af81f2be` 保留。

`target` 仍用于后续验证，当前阶段尚未打包收尾；完整验证后按 AGENTS.md 清理并记录实际释放空间。正式安装未更新，仍为 1.2.4。

## 本域表达式与硬件资源阶段（2026-10-28）

本阶段补充当前镜像的局部/全局混合表达式和 Watch，包括 `$r0 + debug_domain_id`。辅助 GDB 使用实际物理顶层整数寄存器和只读内存，再展开到用户选择的调用者帧；返回信息标明 `register_frame = 0`、选中帧和 `frame_unwind_verified`。表达式执行后再次通过物理 GDB 查询调用者局部值，证明选中帧未被辅助查询改变。其他镜像的变量根限定为全局作用域，保留类型、成员名、已有命名空间和布尔/空指针字面量；拒绝借用当前核寄存器查看其他镜像。

首次实板验证发现直接将调用者的返回 PC 当作辅助 GDB 顶层帧会丢失正确词法作用域：物理 GDB 可以读取 `handled`，辅助会话却报 `No symbol "handled"`。改为从真实顶层帧展开后修复。失败证据 `bao-image-symbols-hardware-1793160356201-867095ed` 保留；后续回归覆盖 Bao、VM0、VM1 及 VM0 调用者帧的表达式和 Watch。

真实硬件资源测试还复现了普通 Continue 和返回 Guest 的状态缺陷：GDB 先发出 `^running`，之后输出 `Cannot insert hardware breakpoint`，却没有 `*stopped`，界面一直显示 RUNNING。`bao-images-resources-hardware-1793162302042-dc0a0ef3/resource-failure-observed.json` 记录该失败，32 个逻辑断点并不代表硬件有 32 个槽位。新增处理只接受 GDB 后端诊断，在事件循环中查询实际线程状态；确认停止后才恢复 STOPPED。返回操作标为 failed，并只移除自己的临时断点。目标程序打印相似文本不会触发该处理。

修复后的资源回归验证：保留全部原有断点、普通 Continue 的异步失败恢复、core0 资源耗尽时 core1 仍能使用硬件断点，以及删除测试断点后再次返回 Guest 成功。日志中恢复只有只读状态查询和自有临时断点删除，没有新增 halt、复位或重连。准备阶段曾因只运行一个 Bao 核、或将对端 Guest 停在初始化处而无法满足同步/邮箱握手条件，已修正测试驱动；这些失败夹具也保留，没有放宽等待超时。

同一 EXE 的本轮实板/真实 ConPTY 报告共 **59 项通过、0 失败、0 跳过**：

| 范围 | 报告 |
|---|---|
| 真实资源耗尽、普通 Continue、每核资源与恢复 | `bao-images-resources-hardware-1793162785278-ff5da315/report.json`：6/6 |
| 三镜像值、混合表达式、调用者 Watch、数据断点及保存恢复 | `bao-image-symbols-hardware-1793162865146-23a18fb2/report.json`：14/14 |
| HVC/IRQ 部分保存、恢复中间阶段及 ERET | `bao-images-edges-hardware-1793162915387-b6605e8d/report.json`：7/7 |
| 双核、20 次真实 HVC 往返、物理/虚拟 IRQ、混合域 | `bao-images-hardware-1793162938955-689d5d88/report.json`：13/13 |
| Files、Symbols、反汇编、运行中浏览 | `bao-images-navigation-hardware-1793163046924-46eaf2e0/report.json`：5/5 |
| 运行中 Watch 分核/镜像归属及旧 ELF 失效 | `bao-images-live-hardware-1793163060797-a8983711/report.json`：5/5 |
| 真实终端 Guest 视图、返回、Watch、源码浏览 | `bao-images-terminal-20261028-125119-7c238860/report.json`：9/9 |

本轮 EXE 保留为 `bin/debugtui-bao-expression-development.exe`，SHA256 `7ead87bfa99583c80e9fc07c7832d2daf334055c0fe65e13d56dd9a9b2d87f0c`，178,548,227 字节，开发版本号 1.2.4。未替换正式安装或发布新版本。Bao/Guest 的 ELF 与 BIN 保持上一阶段摘要，未重新下载固件。

全目标仍未验收：真实执行期间的取消/暂停、返回途中实际异常的优先级等负面路径仍需验证，之后完成统一 A–K 审核；上述通过数量不替代这些尚未完成的项目。软件回归、静态检查及清理结果另记在本阶段 `bao-expression-development-74dec8a7` 清单。

## 双核运行中取消与最终 A–K 验收（2026-10-28）

本节为当前交付状态。前文保留各阶段的失败和进展，不能将旧 EXE 的报告算作本轮结果。当前本地 THA6206／Bao GCC／VM0＋VM1 目标的 A–K 分层验证已通过；最终清单、各报告及文件摘要位于 `artifacts/bao-control-development-2ae1893b/stage-manifest.json`。

已测程序：`bin/debugtui-bao-control-development.exe`，178,535,673 字节，SHA256 `cabd83baf723f3ff528d691b6343ff2c7fea2aa20ed944076da2cb5304e2052e`，开发版本号仍为 1.2.4。正式安装及远端 release 未改动。参考工程为 Bao 仓库根目录的 `debug-multi-images.toml`，使用随 DebugTUI 提供的 GDB 14.2.90 和 OpenOCD 0.12.0+dev-gd3ebb8d-dirty；工具 EXE 摘要另存 `debug-tool-identities.json`。

### 运行中取消的根因与修复

实际复现：GDB all-stop 运行中收到 `-break-delete` 时，先从逻辑表删除断点，再因不能发送远程请求而报 `Cannot execute this command while the target is running`。板上的硬件断点还在，随后会发生没有原断点编号的 SIGTRAP。`always-inserted`、ARM force-mode 和 GDB 15.2.90 实验均未解决；原始失败轮次完整保留。

修复流程核对 `info connections` 的实际当前连接、配置端点、TCL 对端及目标 GDB 端口；禁用并核对自有 GDB 记录后，在同一 OpenOCD 服务内选择正确核心，移除独占物理断点并读回所有地址，最后恢复原目标选择。共址用户断点保留；其条件仍由 GDB 处理。GDB 的禁用记录延迟到下一次真实停止时删除，运行中状态明确标记 `logical_cleanup_pending`。不使用 halt/resume、寄存器改写、重连或复位完成取消。

双核分别通过运行中取消、重复取消、共址 `condition=0` 用户断点、Pause 后重试，以及真实 UDF 异常优先。异常测试确实执行了 UDF 并进入 Hyp undefined 向量，停止类型是该入口的用户硬件断点，不将其写成 GDB 报告 SIGILL。

为制造确定的运行窗口，Bao 增加独立的 `-DebugReturnTests` 构建开关，仅测试模式包含每核等待/故障控制字和 UDF 指令。驱动只通过 AHB 修改这些测试控制字，不改 PC、SP、CPSR 或模式。普通 `-DebugGuestContext` 已恢复、重新下载，控制符号消失；BIN SHA256 与测试前一致：`5f990994aaf0c96d0cc59801e0afd8d5fabe98e44155aa1f93fae697a462c256`。测试固件和普通固件均保留匹配符号及源码归档。

### 本轮证据

下列 **71 项实板／真实 ConPTY 测试，0 失败、0 跳过**，均核对为上述同一 EXE：

| 范围 | `artifacts/` 下报告目录 | 通过 |
|---|---|---:|
| core0／VM0：取消、共址条件、Pause／重试、真实异常 | `bao-images-control-hardware-1793176951373-9ced5f15` | 6 |
| core1／VM1：相同控制回归，对端核心持续运行 | `bao-images-control-hardware-1793176967164-eb22b6b9` | 6 |
| 每核硬件断点耗尽、异步失败恢复 | `bao-images-resources-hardware-1793177093273-59f2c3b6` | 6 |
| 三镜像同名值、调用者表达式、Watch、数据断点、保存恢复 | `bao-image-symbols-hardware-1793177106617-6613bc7d` | 14 |
| HVC／IRQ 部分保存和恢复、有效窗口、ERET | `bao-images-edges-hardware-1793177128305-c917db25` | 7 |
| 两核 Bao／VM、20 次真实往返、物理／虚拟 IRQ、混合域 | `bao-images-hardware-1793177146006-ef5b5329` | 13 |
| Files／Symbols／反汇编、运行中浏览 | `bao-images-navigation-hardware-1793177195419-60111af7` | 5 |
| 运行中 Watch、分核归属、ELF 变化失效 | `bao-images-live-hardware-1793177205117-43080983` | 5 |
| 原生终端 Guest 视图、r 返回、Watch、Source 浏览 | `bao-images-terminal-20261028-164656-dc840f5c` | 9 |

软件层独立记录：生产会话管道模拟目标 `multi-images-software-1793177235141-91160266` 10/10；真实 ARM GCC／DWARF／离线 GDB `image-navigation-1793177064491-fdea8d54` 9/9。原有本机真实 GDB 回归为 Watch 5 组、Memory 3 组和多核控制 13 组，分别在 `watch-tree-1793177350390`、`memory-native-1793177358718`、`multicore-1793177366517`。这些本机组不计入实板数量，其 EXE 摘要来自受控调用记录。

Rust 库测试 606 通过、0 失败、3 个原有忽略；任务／继承／生命周期／驱动集成 7 项通过；clippy all-targets `-D warnings`、fmt、diff 检查通过。首次库测试的一个夹具预置了尚未发出命令的响应，被事件排空取走；已改为逐请求回复，全量重跑通过。首次本机兼容回归误用了不存在的默认 MinGW 路径，在执行产品前失败；改用本机实际路径后通过。失败日志保留，没有放宽等待超时。

### A–K 审核

| 项 | 结果和验证层 |
|---|---|
| A／B | 实板：两个 Bao init，分别进入 VM0／VM1，正确 C 源码与镜像 |
| C | 实板逐指令 HVC→VM_EXIT→分派／处理→VM_ENTRY→ERET；真实 TUI 返回操作 |
| D | 实板物理 timer INTID 30→Bao→虚拟 IRQ→Guest handler→返回；中间保存状态检查 |
| E | 实板三镜像同名函数、全局／局部、调用者表达式和 Watch 树；镜像类型和地址独立 |
| F | 实板读取真实 Guest banked SP／LR 和整数快照，恢复调用位置、栈和局部；部分保存／恢复阶段拒绝旧快照 |
| G | 实板一核 Bao、一核 VM，独立上下文；运行中 Watch 核／镜像／版本失效 |
| H | 实板启动复制前硬件断点、共存、局部／类型地址条件、跨域步进、资源耗尽；双核取消／Pause／重试／异常优先 |
| I | 实板 20 次真实往返；`physical-process-continuity.json` 核对同一两个物理 GDB PID 和同一 OpenOCD PID，断点数稳定 |
| J | 软件会话管道验证非法 ELF、未知／歧义 PC、旧版本及部分加载回滚；真实离线 ARM GDB 验证缺源码／DWARF、映射冲突及重定位。未将这些负例全部称为实板注入 |
| K | 实板保存后新进程恢复镜像断点；原有单 ELF、双本机 GDB、多核控制和生命周期回归通过 |

适用边界保留在使用文档：ARM32 小端 DWARF、只读 C/C++ 表达式子集；不可恢复的优化／浮点值明确不可用；隔离全局赋值和命名全局条件断点仍拒绝，支持已验证的局部／类型地址条件。运行中显示最后已知身份和采样内存，不提供 Trace。镜像在连接前常驻，本轮未宣称支持核心运行中新增或替换 objfile。

临时 WSL 内存配置已在 `finally` 中恢复；当前资源管理器约 0.21 GiB 私有内存，已有约 5.9 GiB 空闲内存，因此没有再次重启。清理限于本轮编译输出和已验证重复夹具；实际释放空间、保留的 EXE、符号／源码归档、全部实板和失败证据见本阶段清单的 `cleanup` 项。
