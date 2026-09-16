# 发布与安装 DebugTUI

GitHub 仓库：<https://github.com/bei-li16/debugtui>。GitHub 保存源码和 Git 历史；npm Registry 保存供用户安装的版本包。Git push 和 npm publish 是两个独立操作。

## 分发边界（0.2 起）

npm tgz 和便携 ZIP 只包含 TUI，不包含 GDB、J-Link 或 tools。DebugTUI EXE 由本地构建产生并打包，始终不提交到 Git。

运行 `./scripts/release-assets.ps1` 构建 TUI 附件；加 -IncludeTools 同时生成可选工具 ZIP，或运行 `./tools/package.ps1` 单独打包工具。两者可以独立更新。已发布的 v0.1.0 附件保留。

## 当前发布方式：GitHub Release + npm URL 安装

安装和升级使用同一个地址：

```powershell
npm install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui
# 卸载
npm uninstall -g @debugtui/cli
```

每次正式发布都保持附件名 debugtui-cli.tgz 不变，将 Release 标记为 Latest；附件内部 package.json 和 EXE 的版本正常递增。GitHub 的 latest 地址自动跳转到最新正式 Release 的同名附件。版本化的 debugtui-cli-<版本>.tgz 同时保留，用于固定版本安装或回退。

发布步骤：

1. 更新版本、完成测试；执行 ./scripts/release-assets.ps1 -IncludeTools，得到版本化 TUI ZIP、版本化 tgz、固定名称 tgz、可选 tools ZIP 和 SHA256SUMS.txt。
2. 提交源码和发布脚本，推送提交及对应 v<版本> 标签。bin、target、artifacts 不提交。
3. 创建草稿 Release，上传 release-assets.json 中 assets 列出的全部文件及 SHA256SUMS.txt，填写发布说明，再发布为 Latest。后续版本也必须上传固定名称 tgz。
4. 发布后执行 ./scripts/test-release.ps1 -PreviousVersion <上一版本>，从公网 latest 地址验证旧版升级、重复安装、版本、校验和及卸载。

URL 安装不要求用户登录 npm，也不要求该包名已注册到 npm Registry。此模式的升级方式是重新执行上述 install URL 命令；Registry 的 @latest 与 npm update 是另一套机制。

以下章节用于将来选择直接发布到 npm Registry 时操作，当前 Release 分发不需要执行 npm publish。

## 1. 账号与包名

先在 npmjs.com 注册账号，验证邮箱并启用双因素认证（2FA），然后在项目目录执行：

```powershell
Set-Location G:\Data\GitFiles\DebugTUI
npm.cmd login --registry=https://registry.npmjs.org/
npm.cmd whoami --registry=https://registry.npmjs.org/
```

GitHub 的 `bei-li16` 与 npm 账号独立。当前 `@debugtui/cli` 是暂定包名，只有拥有 npm `debugtui` 用户/组织作用域的发布权限才能使用它。首次发布建议用自己的 npm 用户作用域：

```powershell
$npmUser = npm.cmd whoami --registry=https://registry.npmjs.org/
if ($LASTEXITCODE -ne 0) { throw '请先完成 npm login' }
npm.cmd pkg set "name=@$npmUser/debugtui"
```

例如 npm 用户名确实为 `bei-li16` 时，包名就是 `@bei-li16/debugtui`。包名包含作用域不影响命令名：安装后仍执行 `debugtui`。

`package.json` 已配置 GitHub repository、homepage、issues，以及 `publishConfig.access=public` 和官方 Registry。源码沿用仓库的 Apache-2.0；第三方工具条款见 `NOTICE`。

## 2. 打包、安装验证和发布预演

首次发布现有已测试的 0.2.0，在本地发布构建仍存在时可以跳过重编译：

```powershell
.\scripts\package.ps1 -SkipBuild
.\scripts\test-npm.ps1

$pack = (Get-Content .\artifacts\npm-pack.json -Raw | ConvertFrom-Json)[0]
$tgz = Join-Path .\artifacts $pack.filename
npm.cmd publish $tgz --dry-run --access public --registry=https://registry.npmjs.org/
```

包名改变后，重新打包会生成对应的新文件名；从 `npm-pack.json` 读取名称，避免发布旧包。测试脚本跟随当前 package.json 的名称与版本，不写死作用域。`--dry-run` 仅预演，不上传包。

从新检出的源码开始、或改了源代码/版本时，先构建再打包：

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\generate-notices.ps1
.\scripts\package.ps1 -SkipBuild
.\scripts\test-npm.ps1
```

构建环境参见 README。开发者需要 Rust 和链接器，最终 npm 用户不需要这些构建工具。构建变更涉及调试链路时，另运行 README 中的实板测试。

## 3. 正式发布

检查预演结果后，发布同一个已测试的 tgz：

```powershell
npm.cmd publish $tgz --access public --registry=https://registry.npmjs.org/
```

按 npm 提示完成 2FA。交互发布使用自己的账号登录即可；不要把账号密码、验证码或 token 写入项目文件。

验证 Registry 中的版本与安装入口：

```powershell
$name = (Get-Content .\package.json -Raw | ConvertFrom-Json).name
npm.cmd view $name version --registry=https://registry.npmjs.org/
npm.cmd install -g "$name@latest" --registry=https://registry.npmjs.org/
debugtui --version
debugtui --demo
```

发布后的包页面为 `https://www.npmjs.com/package/<完整包名>`。当前包只支持 Windows x64。

## 4. 后续升级

每次发布使用新的版本号，例如 `0.2.0` → `0.2.1`。同时更新 `package.json` 和 `Cargo.toml` 的版本，重新构建、测试、打包、发布。打包脚本会核对 npm 版本与 EXE 的 `--version`，避免发布新版本号却仍携带旧 EXE。已发布的同名同版本不能覆盖。

用户退出正在运行的调试程序后，通过以下命令升级；项目配置和日志存放在安装目录之外，不会随安装替换：

```powershell
npm.cmd install -g @你的npm用户名/debugtui@latest
```

版本回退使用明确版本号，例如 `@你的npm用户名/debugtui@0.2.0`。

## 参考

- [GitHub：最新 Release 与固定附件下载地址](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases)
- [npm install：从 URL 安装压缩包](https://docs.npmjs.com/cli/v11/commands/npm-install/)
- [npm：创建及发布带作用域的公开包](https://docs.npmjs.com/creating-and-publishing-scoped-public-packages/)
- [npm publish：tgz、dry-run 和版本限制](https://docs.npmjs.com/cli/v11/commands/npm-publish/)
- [npm 发布的双因素认证要求](https://docs.npmjs.com/requiring-2fa-for-package-publishing-and-settings-modification/)
