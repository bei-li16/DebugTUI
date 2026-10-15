# DebugTUI 1.2.4：芯片策略约束 Probe，补齐 ETM/TPIU 探测与统计

Windows x64 正式版本，标签 `v1.2.4`，从 `main` 发布。

## 修复内容

Probe caps 使用当前选中并完成继承的寄存器描述，执行读取前检查访问权限、读副作用、组件归属及使能/存在条件。芯片描述禁止或未证实可读的寄存器保留原因，且不发送读取请求；F429 的 MVFR2 不再被内置核探测绕过。R52 的架构读取回退同样遵守当前描述策略。

描述格式新增通用 `discovery` 规则，支持声明有界的探测寄存器和基于新鲜位域读值的能力事实。F429 描述通过 ROM 表、CoreSight 组件身份及 ETM/TPIU 能力寄存器确认支持范围。芯片地址、识别条件和策略保留在 TOML；Host 执行通用规则。条件失效后撤销旧探测证据，协调器保留所选芯片目录和探测结果。

Probe 和读取结果提供状态、原因、采样来源、未执行 I/O 数量及去重后的实际请求/响应数量。状态窗口分别显示本次能力探测和当前样本；不将寄存器定义数量冒充实际请求数。没有可用 reader 的 ETM LAR 明确显示为不支持。写专用、读副作用、未实现范围和未开启集成测试模式的访问保护继续生效。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.2.4
```

固定本版可将 `latest/download` 换成 `download/v1.2.4`。便携用户使用完整 `debugtui-1.2.4-win-x64.zip`。F429 系统寄存器描述位于包内 `profiles/registers/stm32f429.toml`，由芯片描述自动关联；工程配置和包外用户描述在升级时保留。

## 验证范围

升版前修复已完成离线回归、严格 Clippy 和 FreeRTOS_Project 实板验证：617 项定义中 423 项自动读取有效，18 项未读取，170 项不支持，6 项因功能未开启而不可用，0 项读取错误。其中 169 项不支持源于硬件未实现，另 1 项为 ETM LAR 缺少受支持 reader。能力探测包含 36 项定义、35 次实际请求和响应；SCB 31 项均有效。上述数字属于该板卡当前固件、使能状态和调试环境，不能作为所有目标的固定结果。

1.2.4 发布验证重新执行库及相关集成回归、格式检查、Release 构建和真实 1.2.3 升级分发检查。发布后从公网 Latest 地址核验安装包，以全局安装版本连接 FreeRTOS_Project，复核 Probe caps、ETM/TPIU、SCB、状态统计和快速搜索。最终测试范围、结果、二进制摘要及清理记录以附件 `validation-1.2.4.json` 为准。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.2.4.tgz`：固定名称和版本化 npm 包。
- `debugtui-1.2.4-win-x64.zip`：包含 ARM GDB、OpenOCD、描述文件和许可证的便携包。
- `debugtui-windows-x64-1.2.4.exe`：独立程序，需另备工具与描述文件。
- `corresponding-source.zip`：修改版 OpenOCD 的对应源码，与随包 PROVENANCE 摘要一致。
- `validation-1.2.4.json` / `SHA256SUMS.txt`：验证摘要和附件校验和。

MVFR2、DWT PCSR、SCS PID5–7 和 ITM LSR 的可读性证据仍不足；ETM/TPIU 集成测试寄存器保留模式保护。本次验证不构建或烧写固件，不复位、不改写系统寄存器、不启用 Trace 或集成测试模式。三个可选 THA6 SVD 本地未提供，本版不附带。测试报告、原始上板证据、正式附件和对应源码保留，构建及分发验证中间文件在验证结束后清理。
