# Bao 与每核 VM 的多 ELF 调试

同一物理核心的 Bao 与 Guest ELF 常驻 GDB。停止后，根据可执行 section、PC 和停止前程序 CPSR 识别镜像；源码显示切换不重连、不复位、不重新下载，也不为识别镜像额外暂停运行中的核心。

本地可用配置为 Bao 仓库根目录的 `debug-multi-images.toml`，连接 core.0/core.1，分别加载 Bao+VM0、Bao+VM1。旧单 ELF 工程不用增加 images。

安装 1.3.1 后，在 Bao 工程目录直接运行 `debugtui --project .\debug-multi-images.toml`。安装及版本核对见 [1.3.1 说明](release-1.3.1.md)；完整多 ELF 实板基线自 1.3.0 建立。

## 配置镜像

Setup 的 **Symbol images** 管理镜像。1.3.2 默认展开 ID、ELF、核心和 Hyp/Guest 运行域；**Advanced settings** 展开源码根覆盖、重定位偏移、源码映射和 Global memory channel。默认不必重复填写 Source root：镜像未设置或留空时继承首页 Source root。已有显式设置继续保留；清空覆盖可恢复继承。其他工程的 VM 源码可以指定不同根目录，路径相对项目 TOML。

选中文本字段按 Enter，出现 `▏` 光标和 Editing 提示。直接输入或粘贴，Ctrl+U 清空，Home/End 移动到首尾；Enter 确认本字段，Esc 撤销本字段。长路径窗口跟随光标。**Ctrl+S 或 Save config 会提交当前输入、校验并直接写入工程文件**，不再需要回首页保存第二次。成功返回首页显示 `Saved <项目路径>`；保存失败保留窗口、输入和错误原因。**Apply to draft** 仅更新首页草稿，之后仍需 Save config 或 Start；Cancel 丢弃本次窗口修改。保存不会下载固件、重连或改变运行状态。

修改与首页 Program / ELF 关联的主镜像 ELF 时，两处路径同步；修改首页主 ELF 也同步已有的关联镜像，VM 路径独立保留。高级 section 地址和诊断提供者仍在 TOML 中配置，界面修改、折叠和保存都会保留它们。修改 Id 后，使用旧 `@image:` 名称的引用需要相应更新。

```toml
[[images]]
id = "bao"
elf = "bin/tha6xxx/tha6206-dual/bao.elf" # 与 program.elf 一致
# 未设置 source_root，继承首页 Source root
domain = "hyp"
memory_channel = "ahb"

[[images]]
id = "vm0"
elf = "build/tha6xxx/tha6206-dual/guest/guest0.elf"
# 未设置 source_root，继承首页 Source root
cores = ["core.0"]
domain = "guest"
memory_channel = "ahb"
```

VM1 使用自己的 ELF 和 `cores = ["core.1"]`。路径相对工程 TOML；源码映射兼容 `/` 和 `\`。按运行地址链接的参考 Guest 偏移为 0；其他布局可使用 offset 和 sections，主 ELF 必须保持链接地址。

工作区 `:images` 查看当前核的镜像和识别状态。运行时显示最后一次停止的身份，不提供连续 Trace。地址重叠且没有足够身份信息时显示 ambiguous；修改同一路径 ELF 后标记 stale，拒绝旧符号的类型求值和写入预览，先确认板上固件匹配再明确重启符号会话。

## 源码浏览、搜索和反汇编

Files 和 Symbols 分别查询本核的每个 ELF，结果带 `@bao:`、`@vm0:` 等归属；相同源文件或函数名不会合并成另一个镜像的结果。Symbols 可输入 `@vm0:guest_main` 限定镜像。源码标签显示 `[镜像] 文件名`，各镜像独立记住浏览位置；`:open @bao:src/arch/armv8/aarch32/vm.c` 只打开源码，不切换物理栈帧或 CPU。标题分别显示 Exec、Browse 和 Frame；运行时标记 last known。

每个 `[[images]]` 下可有自己的 `[[images.source_map]]`，例如：

```toml
[[images.source_map]]
from = '/ci/shared'
to = 'local-vm0'
```

该规则属于它前面的镜像，目标路径相对工程 TOML。同一个远端前缀可以在另一个镜像下映射到不同本地目录；相同前缀优先使用镜像规则，随后使用公共规则，按最长前缀匹配。关闭 Source remap 会同时停用镜像规则和公共规则。兼容 `/`、`\` 及混用；匹配的本地文件缺失时报告缺失，不转而打开另一个镜像的同名文件。相对 DWARF 文件名通过该 ELF 的编译目录记录定位，多个记录无法唯一定位时不猜测。

Files、Symbols 和 ELF 反汇编可在运行时使用，查询不访问物理调试端口。停住时 Asm 默认使用当前镜像的物理 GDB 反汇编；`:disasm @vm0:guest_main` 浏览其他镜像时使用 ELF 文件并标明来源。运行时默认 PC 来自最后一次停止，显示 last-known PC；ELF 指令不是运行轨迹，也不能证明板上代码字节一致。未知或歧义的停止位置不展开物理栈、不显示旧局部值；仍可显式浏览文件和 ELF 指令。

辅助 GDB 同步工程 `gdb.init` 中独立的 `set architecture armv8-r` 等架构声明，避免把 HVC、ERET 等指令按旧 ARM 架构误解码；不会执行其中的复位、下载或连接命令。参考 Bao 诊断构建还给 Hyp 向量和异常汇编提供明确的 CFI 结束边界，禁止 GDB 在保存尚未完成时猜测普通 C 调用栈。该 CFI 位于调试节，不改变装载到板上的程序字节。

## 同名符号、Watch 和断点

普通名称以选中的物理栈帧镜像为默认归属。需要明确查看其他常驻镜像的全局变量时，在 Watch 输入 `@bao:debug_domain_id`、`@vm0:debug_domain_id` 等；只能选择当前核配置的镜像。Watch 行显示 `[bao]` / `[vm0]`，结构体和数组成员保留同一归属。当前物理帧的局部变量继续使用该帧寄存器，挂起 Guest 的局部变量通过独立 Guest 视图查看。

THA6206 的 MPU 会使当前 CPU 内存通道在另一个域中读不到数据；GDB 多个 objfile 也可能把同名全局绑定到主 ELF。1.3.0 对镜像表达式使用单 ELF 的辅助 GDB 获取类型，并通过 `memory_channel` 指定的通道读取实板内存；为空时可以沿用该镜像 `saved_context.channel`。通道来自工程/芯片的 Memory channels 配置。未配置可用通道会明确报错。

目前隔离求值支持带 DWARF 的 ARM32 小端 ELF，限于只读 C/C++ 子集。当前执行镜像支持 `input + debug_domain_id`、`$r0 + debug_domain_id` 等混合表达式及 Watch：从物理 GDB 读取实际顶层整数寄存器，辅助 GDB 通过真实栈内存展开到所选调用者帧，并核对帧地址。不会将调用者返回 PC 冒充顶层 PC，也不改动物理 GDB 的帧选择；不可恢复的寄存器仍为 unavailable。

显式查看其他镜像时，变量名限定为该镜像的全局作用域；表达式不会借用执行镜像的局部变量。其他镜像没有当前物理寄存器帧，因此 `@bao:$r0 + debug_domain_id` 这类请求会被拒绝；查看挂起 Guest 的局部值请使用 Guest 视图。浮点寄存器和 GDB 便捷变量不通过隔离适配器提供。隔离表达式的变量赋值尚未实现，会拒绝写入预览，避免转而写到物理 GDB 选错的同名对象。原有单 ELF 工程不改变求值路径。

代码断点编辑器的 **Image** 项可选择归属；也可输入 `:break @vm0:debug_domain_marker`。后台从该 ELF 离线解析到精确地址，再在物理核心设置硬件断点。保存的是镜像 ID 和原名称，重连时重新解析；地址空间重叠时不冒充支持镜像专属硬件断点。命名数据断点限于当前物理帧所属镜像，先使用该 ELF 解析类型及地址。多镜像条件断点可使用当前位置的局部参数或显式类型地址表达式；命名全局条件会拒绝，因为普通 GDB 条件缺少可靠的 objfile 归属。

启用已有 `[live_watch]` 后，运行时的 8/16/32 位全局标量轮询使用每个镜像的通道、核心和内容版本；通道必须声明 `while_running = true`。`@vm0:变量` 明确绑定 VM0；普通名称使用最后一次停止时的镜像归属。显示 `last-known` 表示该绑定来自已知上下文，数值从内存持续读取，不代表实时执行轨迹。切核、停止代次改变或 ELF 内容变化后，旧结果会被丢弃；ELF 变化会停止使用该旧地址，不自动装载新符号或暂停 CPU。

Bao GCC 参考配置启用了 DWARF unwind，并对汇编异常边界提供仅调试用的 CFI，物理栈在不可恢复的异常边界结束，Guest 栈另用真实快照恢复。参考配置及辅助全局 GDB 使用 `trust-readonly-sections on` 从匹配 ELF 获取只读代码，避免为了代码探测触发 Guest MPU abort；可变数据始终读取实板。使用前仍须核对 ELF 与下载固件一致，ELF 的缓存版本本身不能证明固件一致。

## 在 Bao 内查看挂起的 Guest

需要诊断版 Bao。构建、下载仍使用现有脚本：

```powershell
pwsh -NoProfile -File scripts/tha6206-build.ps1 -Config tha6206-dual -DebugGuestContext
pwsh -NoProfile -File scripts/tha6206-download.ps1 -Config tha6206-dual
```

`DebugGuestContext` 使正常异常入口在 VM_EXIT 完成后捕获真实 Guest banked SP/LR、r0-r12、返回 PC、保存 CPSR、CPU/VM/vCPU 身份；VM_ENTRY 前使其失效。它不填充原先被跳过的 x[13]，也不在调试器中修改 CPU 模式或寄存器。该选项会增加诊断 RAM 和异常路径指令，发布固件可不启用。

在对应 Guest 镜像下显式配置 ABI 和允许读取的内存范围，例如参考 VM0：

```toml
[images.saved_context]
provider = "bao_v1"
symbol = "bao_debug_guest_contexts"
index = 0
cpu = 0
vm = 0
vcpu = 0
channel = "ahb"
memory = [
  { start = 0x10020000, size = 0x8000 },
  { start = 0x10030000, size = 0x1000 },
]
```

VM1 的 index/cpu/vm 为 1，私有 RAM 从 `0x10028000` 开始、大小 `0x8000`。这些值来自当前参考固件，其他布局应按实际 ELF、VM 配置和快照核对。

暂停在 Bao 的有效 HVC/IRQ 保存窗口内，输入 `:guest vm0`（只有一个候选时可直接 `:guest`）。界面单独显示保存 PC/SP/CPSR、Guest 源码、栈和局部变量。↑↓ 或点击选 Guest 逻辑帧，PgUp/PgDn 浏览源码，Esc 回到物理 Bao 界面。切换核心、停止代次或运行状态后旧视图失效，物理 GDB 的选中帧不变。

当前附带 GDB 没有 Python；DebugTUI 启动同工具链的辅助 GDB，通过本机只读 RSP 提供真实保存寄存器、通过已配置 AHB 通道提供内存。辅助 GDB 不连接板上物理调试端口；不接受继续、单步、寄存器/内存写入或断点命令。无需外部 Python 服务、VS Code 或额外安装。

优化导致不可恢复的变量显示 `<optimized out>`；只捕获整数寄存器和 CPSR，缺少的浮点上下文不会补造。无有效快照、错误身份、失效保存阶段、越界 SP、ELF 改变等均拒绝解析。

## HVC 返回 Guest

`:guest-return vm0`，或 Guest 查看界面内按 r，在当前核心设置一个临时硬件返回断点后继续。途中真实断点、异常或用户暂停都会中断该操作，保留真实停止原因，移除本操作的临时断点，不自动越过其他停止。

`:guest-return-cancel` 取消本操作，不额外暂停或继续目标。GDB all-stop 在运行中不能发送删除硬件断点的远程请求，因此 DebugTUI 先核对当前 GDB 连接与 OpenOCD 核心端口，再禁用自有临时断点，通过同一 OpenOCD 服务删除其物理地址，并读回验证。与用户断点共用的物理地址保留，用户条件不变。禁用的 GDB 记录在下一次真实停止时删除；运行中的结果以 `logical_cleanup_pending` 标明这段收尾状态。没有可靠的 TCL/核心映射时会明确报错，不会为取消自动暂停。当前操作限于完整保存的 HVC32 边界。

运行中取消、共址条件断点、显式暂停后重试和真实异常打断已在 THA6206 测试固件上验证。测试固件的 `-DebugReturnTests` 仅用于制造可观察的等待窗口与实际 UDF，不是日常调试所需配置；普通 `-DebugGuestContext` 构建不包含这些控制逻辑。各阶段产物与回归范围见 `bao-multi-elf-development.md`。

部分 GDB 在硬件断点耗尽时先回复 `^running`，随后才报告插入失败，且没有 `*stopped`。1.3.0 收到后端的这类错误后通过只读线程查询确认实际状态；确认已停止才恢复 STOPPED，并清理本次返回操作的临时断点，保留其他断点。普通 Continue 使用相同的状态恢复，不为处理错误额外 halt、复位或重连。

## 自动化与限制

Headless 查询 `images`，或 `guest_context` 参数 `{ "image": "vm0", "level": 1, "expressions": ["handled", "tick"] }`。结果含物理停止上下文、镜像版本、保存序列、只读来源、栈和优化不可用值；内存结果不是跨核原子快照。

普通 Guest 源码断点在 Bao 复制镜像前必须使用硬件断点；多镜像工程默认使用硬件代码断点，资源不足由后端明确报错。THA6206 的 A–K 分层验证已完成，各构建的实际测试范围和摘要分别记录在 [验收记录](bao-multi-elf-development.md)。

实现依据：GDB 的 [多符号文件命令](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Files.html)、[RSP 包定义](https://www.sourceware.org/gdb/current/onlinedocs/gdb.html/Packets.html)、[ARM 核寄存器描述](https://www.sourceware.org/gdb/current/onlinedocs/gdb.html/ARM-Features.html)。
