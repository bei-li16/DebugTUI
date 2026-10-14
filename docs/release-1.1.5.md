# DebugTUI 1.1.5：STM32F429 寄存器规则与自动读取修复

发布日期：2026-10-14。Windows x64 正式版本，标签 `v1.1.5`，从 `codex/register-state-fixes` 发布为 Latest。以下为相对于 1.1.4 的更新。

## 更新内容

- 修复自动读取资格检查：寄存器及别名的完整依赖链必须具有受支持的 reader。仅有架构可读权限、存在性事实或别名关系，不会把缺少读取后端的对象纳入自动读取计划。
- 支持描述型寄存器位置与独立的 `write_fields` 写载荷位域，使只写接口和缺少读取通道的硬件可读接口保留真实权限说明；加载校验拒绝把描述信息当成可执行读写授权。
- 修订 STM32F429 系统寄存器的访问权限、读写规则和位域解释，涵盖 PPB、CONTROL/FAULTMASK、DHCSR、SHCSR、MPU、故障地址、ETM、组件标识及相关勘误。针对本轮 17 项问题，15 项修复、2 项修复并保留文档证据限制；原 31 项 F429 问题复核为 21 项修复有效、10 项在明确限制下有效。
- npm/ZIP 新增 `profiles/registers/stm32f429.toml` 及加载说明，与审计后的参考 TOML 字节一致。展开为 617 项定义、1464 项普通/读取位域及 32 项写入载荷位域。本轮没有新增寄存器或 writer；16 个既有整数 GDB writer 的范围保持原定义。
- 随源码交付 R52/F429 参考描述、审计与复审材料；这些材料的存在不代表全部 212 项问题均已修复。R52 本轮验证限于加载兼容性。
- 分发测试增加安装包内 F429 目录与参考目录的完整展开比较，并保留真实旧版升级、用户配置保护及五个内置核心目录检查。安装包只收录正式 `bin/debugtui.exe`，排除本地备份 EXE。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.1.5
```

固定本版可将 `latest/download` 换成 `download/v1.1.5`。便携用户完整解压 `debugtui-1.1.5-win-x64.zip` 后运行 EXE。npm/ZIP 包含既有 GDB/OpenOCD、芯片/探针配置、SVD、许可证和说明；独立 EXE 附件不带这些资源。

启用芯片专用描述，请按 [STM32F429 目录说明](stm32f429-registers.md) 显式选择随包的 `stm32f429.toml`。默认 `cortex-m4` 仍是通用核心目录。现有工程、自定义寄存器目录和用户配置保留，升级不会自动替换用户的目录副本。

## 验证与限制

发布验证记录于 [TESTING.md](../TESTING.md) 和附件 `validation-1.1.5.json`，包括离线回归、release 构建、完整包目录加载和真实 1.1.4 升级。发布后核对公网 Latest 下载、摘要、重复安装及卸载行为。

本轮没有执行新的实板测试。组件标识跨页布局和 DWT FUNCTION 文档冲突继续保留说明；RW、reset、source 和 confidence 元数据不证明芯片实际权限、写入成功或独立物理回读。没有增加新的寄存器写入后端。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.1.5.tgz`：固定下载名称和版本化 npm 包。
- `debugtui-1.1.5-win-x64.zip`：完整便携包。
- `debugtui-windows-x64-1.1.5.exe`：独立程序。
- `corresponding-source.zip`：随包修改版 OpenOCD 对应源码，摘要与 PROVENANCE 一致。
- `validation-1.1.5.json` / `SHA256SUMS.txt`：验证摘要与附件校验和。
