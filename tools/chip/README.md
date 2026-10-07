# 芯片 SVD 文档

此目录集中保存各芯片的 CMSIS-SVD 外设寄存器描述。`tools/install.ps1` 把它复制到工程的 `.vscode/chip/`，并在工程 `debug.toml` 中写入：

```toml
[program]
svd = "./.vscode/chip/STM32F429.svd"
```

路径相对于 `debug.toml` 所在目录。本分支的 DebugTUI 也支持在 profile 的 `[program]` 中设置 SVD 默认值，路径相对于 profile 所在目录，工程设置优先，空字符串可禁用 SVD；DebugTUI 0.10.0-readonly.1 及更早版本会拒绝含 `[program]` 的 profile，所以随附 profile 暂不设置。

## 当前文件

| 文件 | 芯片 | 版本 | CPU | 外设数量 |
| --- | --- | --- | --- | --- |
| [STM32F429.svd](STM32F429.svd) | STM32F429 | 1.9 | Cortex-M4 | 84 |

`STM32F429.svd` 于 2026-10-07 从本机 STM32CubeIDE 1.18.1 的 `com.st.stm32cube.ide.mcu.productdb.debug_2.2.100.202502251557` 插件内 `resources/cmsis/STMicroelectronics_CMSIS_SVD/` 复制，保留原始字节、版权和许可声明。

- 大小：2,118,594 字节。
- SHA-256：`2B7DE1E383EE415F45339B776942FE01F7B48215316629C3CC280E12661C2400`。
- 许可：[Apache License 2.0](licenses/Apache-2.0.txt)。

`tools/package.ps1` 将整个 `chip/` 目录纳入工具包；`tools/install.ps1` 将其按相同结构复制到工程的 `.vscode/chip/`。
