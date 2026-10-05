# 固定后端源码与构建可用性自检

2026-10-06。REG-006 的源码/构建可获得性已验证；实际工具安装与板级能力另由 REG-001/003/008/505 跟踪。

上游为 [openocd-org/openocd](https://github.com/openocd-org/openocd)，固定提交 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e`。`tools/openocd-adapter/source.lock.json` 固定补丁、十一项 patched source SHA256、Jim Tcl 和七个独立协议；Windows USB/HID/J-Link依赖、许可与固定归档另在 `windows-dependencies.lock.json`。修改清单及协议行为在 [后端说明](../tools/openocd-adapter/README.md)，包含生产源码、独立事务/编码测试、构建与打包配方。

本轮从固定Git对象导出全新源树并重新应用VFP v2当前补丁，十一项源码逐字节匹配源锁，审计 `artifacts/vfp-proof-source-audit.json`。前序Windows审计受系统Git `core.autocrlf=true` 影响生成CRLF，锁校验失败；保留首轮日志和完整源树。按正式Unix构建配方设置 `core.autocrlf=false`/LF后重新审计通过，未放宽哈希或比较规则。前序证据 `artifacts/register-backend-source-audit.json`；原失败日志 `artifacts/register-backend-source-audit-windows-crlf.log`。

| 产物 | 当前证据 |
| --- | --- |
| Linux候选 | 新目录完整autotools/configure/make/install，七套生产事务与dummy原生命令通过；版本0.12.0+dev-gd3ebb8d-dirty |
| Windows x64候选 | WSL中的MinGW-w64 GCC 10 POSIX交叉构建；Windows本机DLL导入、生产事务、七协议、两份F429配置、CMSIS-DAP双后端配置和适配器列表检查通过 |
| 对应源码ZIP | 包含完整固定源码/依赖、Git对象、许可、锁、补丁、构建/测试配方及独立TCP驱动JSON夹具；ZIP完整性及全部十一项源码/recipe/夹具哈希已核对，全新解压后七模型/八TCP/后端命令通过 |
| 运行ZIP | EXE及DLL/CFG/license/PROVENANCE，EXE哈希与本机验证记录一致 |

构建日志为 `artifacts/vfp-proof-windows-build.log`、`vfp-proof-linux-build.log`，七协议/事务/实际命令报告为 `artifacts/vfp-proof-windows-report.json`、`vfp-proof-linux-report.json`。当前Windows EXE SHA256为 `1d6777814205524fc10b0e0aa31599f601a2f949f878ef29d8033f6a36d9ab5e`，Linux为 `f421f9961b585603f0ff28240b76f3502c548513a26a467832b6fa09f912f2e8`；对应源码ZIP SHA256为 `bfd30b72a5c76c488e4243bc247da3f803492713ae1b271285f02f7a35db990c`。本机最终日志 `artifacts/vfp-proof-windows-native-package-regression.log`；Linux最终七套事务/八TCP日志 `artifacts/vfp-proof-linux-native-final.log`。

自检旧源码包发现独立TCP驱动依赖的JSON在工作区外缺失，原包及 `artifacts/vfp-proof-source-package-pre-audit.json` 保留，拒绝日志 `artifacts/vfp-proof-source-package-negative.log`。当前配方将 `tests/fixtures/register-vfp-write-board.example.json` 随包保存并在PROVENANCE记录SHA256，本机验证拒绝缺失或变化。新入口 `tests/source-package.py` 从全新目录执行解压后的配方与夹具；当前七模型/八TCP/dummy命令通过，独立报告 `artifacts/vfp-proof-source-package-report.json`。它不会借用工作区夹具，不连接探针。

Windows包检查十一项通过，日志 `artifacts/vfp-proof-windows-package-regression.log`，新增JSON缺失与变化两项拒绝。首轮既有Git refs负向包未带JSON，被新增检查提前拦截；补齐该夹具的有效JSON后继续独立验证Git空目录拒绝，原九项没有删减，首次错误日志保留。

复建入口需要新输出目录，主机工具链按后端README准备；支持源码缓存的Windows入口可使用源码ZIP中的缓存。Linux配方要求Git/GCC/make/autotools/pkg-config；Windows额外需要Python、MinGW-w64 x64工具和已列出的开发依赖。

```sh
sh tools/openocd-adapter/build.sh NEW_LINUX_DIRECTORY
sh tools/openocd-adapter/build-windows.sh NEW_WINDOWS_DIRECTORY [SOURCE_CACHE]
python3 tools/openocd-adapter/test.py --source PINNED_SOURCE --out TEST_DIRECTORY --openocd BACKEND
python3 tools/openocd-adapter/tests/source-package.py --archive SOURCE.zip --out NEW_DIRECTORY --cc GCC --openocd BACKEND
```

后端版本号0.12.0本身不能证明这些扩展可用；必须匹配源锁规定的独立协议、各reader最低能力及实际候选哈希。dirty表示尚未进入上游的受控适配补丁。构建时间/路径/工具链影响文件字节，不声明二进制逐字节可复现。两个候选当前均未安装；没有实板结果或最终工具集/Release交付声明。源码/补丁/GPL许可随未来交付保留。
