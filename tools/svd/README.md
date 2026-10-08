# 芯片 SVD 文档

此目录集中保存 CMSIS-SVD 外设寄存器描述。芯片调试参数位于独立的 `../devices/`；例如 `devices/stm32f429.toml` 引用：

```toml
[program]
svd = "../svd/STM32F429.svd"
```

路径相对于声明它的芯片 TOML 文件。工程中不设置 `program.svd` 时随 Chip 自动选择；显式工程值优先，空字符串表示禁用。默认使用安装包中的 `tools/svd/`，工程不写死 SVD 路径。

## 当前文件

| 文件 | 芯片 | 版本 | CPU | 外设数量 |
| --- | --- | --- | --- | --- |
| [STM32F429.svd](STM32F429.svd) | STM32F429 | 1.9 | Cortex-M4 | 84 |
| [THA6104.svd](THA6104.svd) | THA6104 | draft | Cortex-R52 | 127 |
| [THA6206.svd](THA6206.svd) | THA6206 | draft | Cortex-R52+ | 155 |
| [THA6412.svd](THA6412.svd) | THA6412 | draft | Cortex-R52 | 229 |

`STM32F429.svd` 于 2026-10-07 从本机 STM32CubeIDE 1.18.1 的 `com.st.stm32cube.ide.mcu.productdb.debug_2.2.100.202502251557` 插件内 `resources/cmsis/STMicroelectronics_CMSIS_SVD/` 复制，保留原始字节、版权和许可声明。

- 大小：2,118,594 字节。
- SHA-256：`2B7DE1E383EE415F45339B776942FE01F7B48215316629C3CC280E12661C2400`。
- 许可：[Apache License 2.0](licenses/Apache-2.0.txt)。

三个 THA 文件于 2026-10-08 从本地 `mcal-vsconfig`（提交 `4a24a8df28becd097bb60451c7583fa22da7738c`）的 `THA6104/tha6104.svd`、`THA6206/tha6206.svd`、`THA6412/tha6412.svd` 导入。它们是参考工程附带的 draft 外设描述，源文件未附单独许可声明；上面的 STM32 Apache-2.0 声明不适用于 THA 文件。CPU 类型来自芯片配置，不表示原始 XML 声明了完整 CPU 信息。

| 文件 | 字节数 | SHA-256 |
| --- | --- | --- |
| THA6104.svd | 12,485,257 | `352038FCEC455F1E814708DF1DAED1CE55B4FA261B345050963BD42BF7CF3356` |
| THA6206.svd | 15,190,477 | `7BB2ECED79AB5EF4FADA1E37B128EA862DD0D3C1E279A7D4F82FF80F91E48BBC` |
| THA6412.svd | 26,508,471 | `DFCB146C377EB3B8C2119644856198A816127BB80F34860ED198F6944C590014` |

THA6104、THA6206 保留原始字节。THA6412 原文件根节点的设备名误写为 `THA6206`，这里仅将根节点 `<name>` 修正为 `THA6412`，其余字节保持原样；原文件 SHA-256 为 `C345DA36525D365F9AE291D7E4A72EA2F9F1025A6862F790CFC5AE496E3897B2`。三个文件均使用 DebugTUI 的实际 SVD 解析器检查外设、MAINRESET 地址和 DBGRSTCON.DBGRST 字段，未重新解释厂商 draft 中的寄存器权限或访问副作用。

THA6 系列 SVD 仅在本地保留，由根目录 `.gitignore` 的 `/tools/svd/THA6*.svd` 排除，不提交到 Git。npm 的 `package.json.files` 显式收录 `tools/svd/`，因此分发包仍包含本地存在的文件；Git 忽略不代表 npm 排除。`scripts/package.ps1` 按本地实际文件核对打包清单，不额外检查 THA SVD 是否存在或为空。

npm 包和便携 ZIP 携带此目录及许可证。`scripts/package-tools.ps1` 仍可生成独立工具包；仓库 `tools/install.ps1` 保留旧工程复制到 `.vscode/svd/` 的兼容用法。
