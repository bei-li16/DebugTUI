# STM32 SVD resources

从本机 STM32CubeIDE 1.18.1 的设备数据库复制的原始 CMSIS-SVD 文件，用于后续的外设与寄存器描述功能。

## 内容

- 仅保留 1 个 SVD 文件，共 2,118,594 字节（约 2.02 MiB）。
- 当前 STM32F429 工程对应 [`STM32F429.svd`](STM32F429.svd)：版本 1.9、Cortex-M4、84 个外设描述。
- [`catalog.json`](catalog.json) 提供文件名、设备名、版本、CPU、外设数量、文件大小、SHA-256 和来源插件标识。文件路径均相对于本目录。

原文件未经裁剪或重写；复制后已逐个校验 SHA-256，并检查 XML 可解析性。XML 解析检查不等同于对寄存器定义进行芯片实测验证。

## 来源

复制日期：2026-09-17。

| CubeIDE 插件 | 文件数 |
| --- | ---: |
| `com.st.stm32cube.ide.mcu.productdb.debug_2.2.100.202502251557` | 1 |

来源目录均为插件内的 `resources/cmsis/STMicroelectronics_CMSIS_SVD`。使用这些本地副本不需要安装 CubeIDE，也不依赖原机器的安装路径。

## 许可

保留各 SVD 原始版权和许可声明；文件头中的声明适用于相应文件。随附说明保存在 [`licenses/`](licenses/)：

- `Apache-2.0.txt`：SVD 文件头引用的 Apache License 2.0 全文。
- `CubeIDE-MCU-about.html`、`CubeIDE-MPU-about.html`：来源插件自带说明。
- `CubeIDE-MPU-SVD-License.html`：MPU SVD 目录随附许可文件。

## 使用范围

在 DebugTUI 启动页的 SVD file 中选择本目录的 STM32F429.svd，即可使用 Peripherals 外设寄存器视图。当前 npm 包的文件列表不包含此目录，资源也未嵌入 TUI 可执行文件。
