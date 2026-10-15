# DebugTUI 1.2.3：修复快速搜索寄存器卡住

发布日期：2026-10-15。Windows x64 正式版本，标签 `v1.2.3`，从 `main` 发布。

## 修复内容

在系统寄存器窗口快速提交筛选条件，或连续切换 SCB、NVIC、SysTick 搜索时，界面可能一直等待 `registers_read`，直到后续输入或其他事件才继续。原因是协调器完成当前批次后，仅根据外部消息判断是否休眠，内部队列中尚未执行的请求可能没有新的唤醒信号。

本版在休眠前继续处理内部可执行请求；真实的 worker 等待和组停核等待仍使用原有事件及截止时间。修复不依赖输入延时或定时轮询。新增两个离线回归，分别覆盖异步 worker 回复和同步回复后继续处理筛选、读取及状态请求，并提供真实终端、零按键延时的实板搜索回归脚本。

F429 芯片入口、617 项系统寄存器定义、目录继承及权限规则沿用 1.2.2。SCB、NVIC 和 SysTick 随描述文件提供，读取仍遵守容量、可用性及读副作用限制；SysTick CTRL 的读清除保护继续生效。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.2.3
```

固定本版可将 `latest/download` 换成 `download/v1.2.3`。便携用户使用完整 `debugtui-1.2.3-win-x64.zip`。工程配置和包外用户芯片描述在升级时保留。

## 验证范围

升版前的同一修复实现已经通过离线回归 866 项、0 失败、3 个原有忽略项，以及严格 Clippy；两个新增用例在修复前均失败、修复后均通过。该阶段也通过 F429 实板 Debug/Release 各五次独立连接、各 60 项搜索检查，以及 300 项 MMIO 独立读回比对。上述结果属于升版前验证。

1.2.3 发布验证重新执行库回归、格式检查、Release 构建及真实旧版升级分发检查。发布后从公网 Latest URL 核验安装包，再以全局安装程序连接 FreeRTOS_Project，验证零按键延时快速搜索和 SCB、NVIC、SysTick。最终结果、正式二进制摘要与测试范围以附件 `validation-1.2.3.json` 为准。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.2.3.tgz`：固定名称和版本化 npm 包。
- `debugtui-1.2.3-win-x64.zip`：包含 ARM GDB、OpenOCD、描述文件和许可证的便携包。
- `debugtui-windows-x64-1.2.3.exe`：独立程序，需另备工具与描述文件。
- `corresponding-source.zip`：修改版 OpenOCD 的对应源码，与随包 PROVENANCE 摘要一致。
- `validation-1.2.3.json` / `SHA256SUMS.txt`：验证摘要和附件校验和。

本地未提供三个可选 THA6 SVD，本版不附带。测试报告、原始上板证据、正式附件与对应源码保留；构建、安装验证及打包中间文件在验证完成后清理。
