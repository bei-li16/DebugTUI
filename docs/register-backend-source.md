# 固定后端源码与构建可用性自检

2026-10-06。REG-006 的源码/构建可获得性已验证；实际工具安装与板级能力另由 REG-001/003/008/505 跟踪。

上游为 [openocd-org/openocd](https://github.com/openocd-org/openocd)，固定提交 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e`。`tools/openocd-adapter/source.lock.json` 固定补丁、十一项 patched source SHA256、Jim Tcl 和七个独立协议；Windows USB/HID/J-Link依赖、许可与固定归档另在 `windows-dependencies.lock.json`。修改清单及协议行为在 [后端说明](../tools/openocd-adapter/README.md)，包含生产源码、独立事务/编码测试、构建与打包配方。

本轮从固定Git对象导出全新源树并重新应用当前补丁，十一项源码逐字节匹配源锁。Windows首次审计受系统Git `core.autocrlf=true` 影响生成CRLF，锁校验失败；保留首轮日志和完整源树。按正式Unix构建配方设置 `core.autocrlf=false`/LF后重新审计通过，未放宽哈希或比较规则。证据 `artifacts/register-backend-source-audit.json`；原失败日志 `artifacts/register-backend-source-audit-windows-crlf.log`。

| 产物 | 当前证据 |
| --- | --- |
| Linux候选 | 新目录完整autotools/configure/make/install，七套生产事务与dummy原生命令通过；版本0.12.0+dev-gd3ebb8d-dirty |
| Windows x64候选 | WSL中的MinGW-w64 GCC 10 POSIX交叉构建；Windows本机DLL导入、生产事务、七协议、两份F429配置和适配器列表检查通过 |
| 对应源码ZIP | 包含完整固定源码/依赖、Git对象、许可、锁、补丁、构建/测试配方；ZIP完整性及全部十一项源码/recipe哈希已核对 |
| 运行ZIP | EXE及DLL/CFG/license/PROVENANCE，EXE哈希与本机验证记录一致 |

构建日志为 `artifacts/banked-proof-windows-build.log`、`banked-proof-linux-build.log`，七协议/事务/实际命令报告为 `artifacts/banked-proof-windows-report.json`、`banked-proof-linux-report.json`。当前Windows EXE SHA256为 `802648db51b6c9468a21c8771a034cba695a0983454ff2c608623cd0b5f78a71`，Linux为 `3523c386662f52cf3acd014196eaded7aa298467205fb494d08dd0378066af4f`；对应源码ZIP为 `2a2e00b57c73b1002b7d2a07ac7b6c3d7a9da89bd6e656a9983e773075b20c86`。

复建入口需要新输出目录，主机工具链按后端README准备；支持源码缓存的Windows入口可使用源码ZIP中的缓存。Linux配方要求Git/GCC/make/autotools/pkg-config；Windows额外需要Python、MinGW-w64 x64工具和已列出的开发依赖。

```sh
sh tools/openocd-adapter/build.sh NEW_LINUX_DIRECTORY
sh tools/openocd-adapter/build-windows.sh NEW_WINDOWS_DIRECTORY [SOURCE_CACHE]
python3 tools/openocd-adapter/test.py --source PINNED_SOURCE --out TEST_DIRECTORY --openocd BACKEND
```

后端版本号0.12.0本身不能证明这些扩展可用；必须匹配源锁规定的独立协议、各reader最低能力及实际候选哈希。dirty表示尚未进入上游的受控适配补丁。构建时间/路径/工具链影响文件字节，不声明二进制逐字节可复现。两个候选当前均未安装；没有实板结果或最终工具集/Release交付声明。源码/补丁/GPL许可随未来交付保留。
