# 寄存器目录交付与升级自检（REG-107）

本批核对内置目录与用户扩展位置在 npm、直接 EXE、ZIP 和 `--init-profiles` 的交付行为。只执行初始化、离线本地安装和未连接的 `registers_list`／切核；不启动 GDB、OpenOCD 或探针。目录有定义不证明实际 CPU、reader、可选扩展或 writer 已支持。完整工具集、最终升版、系统安装和公网 Release 仍按 REG-505 等后续任务验收。

## 交付约定与打包检查

三份 M4／R52／R52+ 目录编译进 EXE；npm 与 ZIP 还随包提供原始 TOML、devices.toml 和 install.cjs。生产 `package.ps1` 检查五份必需 profile 文件，缺少任一文件拒绝生成成功记录；`release-assets.ps1` 核对解压后同一载荷的 SHA256。原有独立工具环境边界保持，包内不混入 tools／开发依赖。

直接 EXE 独立运行时不依赖旁边的内置 TOML。软件验收分别使用显式随包文件和内置 CPU 请求，将实际 API 返回的整份 Catalogue 逐字段比较，包括分组、位宽、字段、条件、别名及 reader/writer 元数据，避免只以版本号、文件数量或名称证明二者一致。

`--init-profiles` 创建用户 chip catalogue 及 `profiles/registers/` 扩展位置；空扩展目录保持为空，不复制会长期遮蔽新版本的默认模型。已有客户 devices、tools profile、任意 valid/invalid register 文件、嵌套目录和工程配置均不改写；升级、同包重复安装、卸载及重装也不动外部客户数据。已有无效 override 仍优先报错，不能因升级而偷偷改成内置定义。扩展位置被普通文件占用时明确报错并保留原文件；devices 无效导致 npm postinstall 失败，不把初始化错误当安装成功。

客户配置使用 LOCALAPPDATA/debugtui 或显式 DEBUGTUI_CONFIG_DIR，位于 npm／ZIP 的程序目录之外。包自身的 `profiles/registers/` 是随包模板，用户修改和引用必须放在扩展目录或工程位置；卸载程序不是保存包目录内临时编辑文件的机制。

## 自动化软件证据

`scripts/test-register-distribution.cjs --binary <EXE>` 在含中文／空格的独立源码副本运行真正的生产打包脚本，使用专用 npm cache、private prefix 与 config roots。npm 开启真实 postinstall 并离线运行；每个进程 stdout/stderr、资产 SHA256 和 JSON 报告都保存在独立 artifacts 目录。不改系统实际安装、客户文件或原仓库 bin。

十四项 Case 覆盖：生产 npm／ZIP／EXE／checksums；缺失 profile 的生产门禁；直接 EXE 全部内置模型及空扩展位置；ZIP 的全部模板／内置逐字段一致与非连续核心；重复初始化的客户文件；旧包基线；npm 升级与钩子；CMD／PowerShell 入口和每核 builtin/user 来源；重复安装；卸载；重新安装；EXE 与 ZIP 两种替换升级；损坏文件／同名目录 override 的错误与保留；无效 devices 的 hook 失败。

默认基线是 **0.0.0-fixture 模拟包替换**，使用当前 EXE，不称为历史 Release。它作为 `actual_register_distribution_packages_init_upgrade_preservation_and_multicore_sources_are_verified` 纳入 Cargo 集成。独立 `register_extension_initialization_failure_does_not_replace_customer_files` 覆盖扩展目录被客户文件占用时的初始化失败。

提供 `--previous-package`、`--previous-sha256`、`--previous-version` 时，脚本验证真实旧包摘要、包版本及原 EXE 版本，保留旧 EXE hash，再执行同样十四项流程。历史包必须先对照发布方 SHA256SUMS 校验；不能把离线 fixture 或安装目录截图当作公网旧版升级证明。

本批使用已发布 [v0.9.3](https://github.com/bei-li16/DebugTUI/releases/tag/v0.9.3) 的 `debugtui-cli-0.9.3.tgz`，SHA256 为 `f3a9b1ffe151563c4b678e2a177ad123a03166a70e77aea32b28c73417c07676`。开发代码仍标 0.9.3，因此真实旧包运行验证的是 **同版本、不同二进制与目录内容的替换**；模拟包另行覆盖 npm 版本变化。最终升版后的真实版本升级与公网安装仍需 REG-505 重新验证。本批不发布 Release。

最终完整 Cargo／F24、严格 Clippy、release build 和真实旧包验证报告见 [开发进度](registers-development-status.md)。前序配置、Setup、共享 owner、生命周期和来源回归继续执行。[八项环境验收](../tests/cases/register-distribution.md) 均 SKIPPED，供客户权限、链接／UNC、实际终端和最终版本发布时运行；没有上板。
