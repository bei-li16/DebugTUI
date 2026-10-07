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

`STM32F429.svd` 于 2026-10-07 从本机 STM32CubeIDE 1.18.1 的 `com.st.stm32cube.ide.mcu.productdb.debug_2.2.100.202502251557` 插件内 `resources/cmsis/STMicroelectronics_CMSIS_SVD/` 复制，保留原始字节、版权和许可声明。

- 大小：2,118,594 字节。
- SHA-256：`2B7DE1E383EE415F45339B776942FE01F7B48215316629C3CC280E12661C2400`。
- 许可：[Apache License 2.0](licenses/Apache-2.0.txt)。

npm 包和便携 ZIP 携带此目录及许可证。`scripts/package-tools.ps1` 仍可生成独立工具包；仓库 `tools/install.ps1` 保留旧工程复制到 `.vscode/svd/` 的兼容用法。
