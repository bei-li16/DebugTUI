# DebugTUI 1.3.2：镜像配置保存与参数简化

Windows x64 正式版本，标签 `v1.3.2`，从 `main` 发布。沿用 1.3.1 的 Bao＋VM 常驻多 ELF 调试能力，改进镜像配置的保存与日常使用。

## 镜像配置

- Symbol images 的 **Ctrl+S / Save config 直接保存工程**，包括正在编辑、尚未按 Enter 确认的输入。保存成功显示项目路径；校验或写入失败保留窗口和修改。
- **Apply to draft** 继续只更新草稿，Cancel 丢弃本次窗口修改，避免把“更新草稿”误认为“已经保存”。
- 主镜像与首页 Program / ELF 的路径双向关联；修改 VM ELF 不影响主 ELF 或另一个 VM。
- 默认只展开 ID、ELF、核心和运行域；源码根覆盖、偏移、映射、内存通道收入 **Advanced settings**，折叠不丢失现有设置。
- 镜像 Source root 未设置或留空时继承首页，另有源码目录的 VM 可在高级设置覆盖。已有显式 source_root 保留原语义。
- 保留已有 saved_context、sections 和其他工程配置。保存配置不连接调试器、不下载固件、不重新加载正在使用的符号；Start 才应用到新会话。

使用：Symbol images → 选择镜像及字段 → Enter 编辑 → Ctrl+S。需要暂存而不写文件时选 Apply to draft。详细配置见 [Bao＋VM 使用说明](bao-multi-elf.md)。

镜像中显式 `source_root="."` 仍表示工程 TOML 所在目录，不表示继承首页；在 Advanced settings 清空该覆盖可恢复继承。修改配置不构成运行中 ELF 热替换，Start 才将新配置应用到新的调试会话。

## 安装与使用

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.3.2
```

固定本版可将 `latest/download` 换成 `download/v1.3.2`。便携用户下载完整 ZIP；单独 EXE 需配套工具资源。升级保留包外工程和用户配置。Bao 工程在工程根目录执行 `debugtui --project .\debug-multi-images.toml`，或在 Setup 的 Project 行选择该文件。

## 手册与验证

完整 TeX/PDF 手册同步至 1.3.2，共 103 页，补齐直接保存、草稿/取消、保存失败、Source root 继承和主 ELF 联动说明，更新当前 Setup 入口、默认关闭日志及显式写入 UI 描述。三镜像示例采用继承写法；目录、交叉引用、字体和全页排版已验证。

本版已有 Rust 库测试 615 项通过、0 失败、3 项原有忽略，严格 Clippy 全目标通过；Release 与全局安装版的原生终端镜像编辑各 8 项、持久化各 12 项及多镜像软件 10 项通过。发布分发验证使用校验过 SHA256 的真实 1.3.1 包，覆盖 14 项安装、升级、配置保留及卸载；公网 Latest 的下载、升级、重复安装和 CMD/PowerShell 入口结果见附件 `validation-1.3.2.json`。

THA6206 Bao/VM 的完整实板与终端 74/74 继续引用 [1.3.0 基线](release-1.3.0.md)，不改记为本版新 EXE 的实板回归。本轮改动集中于 Setup 配置，不下载或改写 Bao/VM 固件。多 ELF 在连接前常驻；运行中热替换 ELF、跨镜像赋值和任意挂起 Guest 写入不作为本版能力交付。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.3.2.tgz`：固定名称与版本化 npm 包。
- `debugtui-1.3.2-win-x64.zip`：含工具和许可证的便携包。
- `debugtui-windows-x64-1.3.2.exe`：独立程序。
- `debugtui-usermanual-1.3.2.pdf` / `.tex`：与源码同步的完整手册。
- `corresponding-source.zip`：修改版 OpenOCD 对应源码，与随包 PROVENANCE 摘要一致。
- `validation-1.3.2.json` / `SHA256SUMS.txt`：分层验证摘要和发布附件校验和。
