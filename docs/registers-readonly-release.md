# 0.10.0-readonly.1 只读系统寄存器交付

来自 `codex/register-debugging`，已发布为 [v0.10.0-readonly.1 prerelease](https://github.com/bei-li16/DebugTUI/releases/tag/v0.10.0-readonly.1)，tag 指向 `6ad796246bc94b833cf0565d5833dc193669d22d`。正式 `latest` 仍为 v0.9.3。[冻结验收清单](registers-readonly-goal.md)已完成 36/0；本页记录构建、安装和附件边界，后续完成记录不改变已发布 tag。

## 软件验收

- 最终代码 `5ec0f32ae716088d8118032e06f51d35b145c077`：24 功能套件全部通过；Cargo 761 通过、0 失败、2 项既有 ignored，严格 Clippy 全 target 通过。后续交付记录修改不改变生产代码。
- 优化 EXE 版本 `debugtui 0.10.0-readonly.1`，SHA256 `eac50b06182282124fe7a9b2e4f16d7f76846352ca9466198d5ecba2bcd98a20`。
- 真实 GitHub v0.9.3 历史包 SHA256 `f3a9b1ffe151563c4b678e2a177ad123a03166a70e77aea32b28c73417c07676`，已与该 Release 的 SHA256SUMS 核对。
- 最终优化 EXE 运行生产打包和隔离安装测试：14 通过、0 失败、0 skipped。真实 npm postinstall 执行；从 v0.9.3 升级、重复安装、卸载重装保留客户文件和目录。EXE/ZIP/npm 入口及 M3/M4/M7/R52/R52+ 目录来源一致，损坏覆盖定义和初始化失败按预期报错。
- 软件报告：`C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\evidence\functional-1791299827400-74f89fdd\report.json`；安装报告：`C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\evidence\distribution\register-distribution-1791302156623-b2f8a61e\report.json`。

## 安装及附件

TUI 的 EXE、便携 ZIP、版本化 npm tgz 和固定名称 `debugtui-cli.tgz` 使用同一优化 EXE。ZIP/npm 附带六份目录、只读多核/R52 模板、用户文档和硬件 case 文档，不带环境工具。

可显式安装本预发布：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/download/v0.10.0-readonly.1/debugtui-cli.tgz"
debugtui --version
```

2026-10-07 已按指定 tag 完成公网验收：八个附件均下载，摘要与本地及 GitHub digest 一致；`test-release.ps1 -ReleaseTag v0.10.0-readonly.1 -PreviousVersion 0.9.3` 的旧版升级、重复安装、CMD/PowerShell 入口、配置保留、渲染和卸载通过。该公网脚本跳过安装 lifecycle，实际 postinstall 的客户目录/错误处理已由前述真实旧版 14 项生产分发测试证明，二者包摘要一致。公网报告位于 `G:\Data\GitFiles\DebugTUI\artifacts\public-release-20261007-002020\result.json`，全附件报告位于 `C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\public-release-assets-20261007\report.json`。

修改后的 OpenOCD 作为独立 `openocd-windows-x64-candidate.zip` 附件，必须同时提供 `corresponding-source.zip`。其 PROVENANCE 的对应源码摘要为 `67a14bcc54fbd89337073246bd3cf0e1ffcd5ddcd378bb84c424925676e614e8`；运行 EXE 摘要为 `0820f197803c55ecf756d7b7ef33f6c82ef97821b2764561ad71d32455e779a0`。对应源码内含固定上游、Jim Tcl、依赖源码、补丁、构建配方、测试和许可。当前配方/补丁逐项摘要匹配，独立解包源码的九项 C 事务模型及真实后端命令通过，Windows 本机 DLL/包检查 11 项通过。

Linux 后端已完成软件构建和命令验证；本次分发 Windows x64 程序和 Windows 后端候选。全局正式版、stock xPack 和实际板卡连接配置不因隔离安装测试改变。候选使用和匹配配置见[R52 当前 Debug 权限及事务说明](register-r52-core-read.md)。

## 限制与后续

八个硬件驱动默认 32 项 SKIPPED，未连接板卡或执行 ARM 指令；软件模型不证明物理目标行为，board verified 保持 false。硬件环境就绪后执行[用例清单](../tests/cases/registers-readonly-release.md)。

只读 M3/M4/M7 和已证明当前 Debug EL2 的 R52 常用身份、控制及 MPU 属于本版。R52 低 EL/Guest 证明不足时返回 Unknown/Restricted；R52+ 未适配身份返回 Unsupported。完整 Banked 低 EL、VFP 写入、Bao/Guest 全权限、完整 Timer/PMU/GIC/STM、Trace 和各类写入另列后续里程碑。

三个核心参考保留绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
