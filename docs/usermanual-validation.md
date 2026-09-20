# DebugTUI 用户手册校验记录

校验日期：2026-09-21（Asia/Shanghai）。

## 交付物与分析基线

- `usermanual.tex`：独立 LaTeX 源文件，中文用户手册；21 个正文章节、3 个附录、13 幅原生 TikZ 图。
- `usermanual.pdf`：55 页 A4，557333 字节，未加密，包含目录、书签和链接。
- `build-usermanual.ps1`：XeLaTeX 三遍构建脚本；成功后默认仅保留最终 PDF，失败时保留诊断日志。`-KeepIntermediate` 可用于排版排错。
- 产品版本：0.8.3；源码提交：`5ab08a2e925f01f9cfaccbd098963d3c598d8d3d`。
- 当前 PDF SHA-256：`6f45c8637b6119613408b115fcb5a392457ecc0831a2dc37497e2bd515d67bec`。

PDF 含构建时间等元数据，再次编译的二进制哈希可能不同。上述哈希仅标识本次交付。

## 内容核对范围

检查了全部 35 个 `src/**/*.rs` 文件，以及 Cargo/CLI 配置、根目录工程说明、测试与发布脚本、tools 下的三个调试环境和板级配置、工具版本与校验清单、STM32F429 SVD 和本地历史实板记录。第三方 GDB/OpenOCD 二进制不作为 DebugTUI 自研源码统计。

只读检查关联固件工程 `G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project` 的项目配置、Makefile、链接脚本、FreeRTOS 配置和用户任务。文档示例将其临时断点换成 `main`，没有修改该固件工程。

对照 GDB、OpenOCD、SEGGER、ST 和 Arm 官方资料说明 MI/RSP/TCL、探针协议与驱动、CoreSight/ADI、CTI、Trace 和芯片接口。实际能力仍以本地源码、配置和工具版本为准。手册正文标明实现入口、证据路径、历史版本与当前实现差异。

## 本次内容修订

- 第 8 章：区分产品、主机协议、USB 驱动/后端、探针固件与 SWD/JTAG；说明 CMSIS-DAP HID/bulk、J-Link 两种 Server 路径以及复位/引脚边界。
- 新增第 9 章：DAP/DP/AP、AHB-AP/APB-AP/AXI-AP/JTAG-AP、MEM-AP 访问、ROM table、Cortex-M4 调试资源、F429 AP0 映射、CTI/CTM/ECT、ETM/ITM/DWT/ATB/TPIU/ETB/TMC 及 Trace 采集边界；新增 4 幅图。
- 第 19.3 节：补收 GDB + J-Link Server 约 52–53 秒后的长会话故障。核对四组原始 trace 的 52.522/53.325/52.575/52.893 秒时间戳，以及 30 秒/两组 60 秒静置结果 JSON；未把汇总 elapsedMs 当作首次异常时间。
- 记录用户对 clone 探针的怀疑，但没有将其写成已确认根因或固定反 clone 超时机制；明确区别“目标访问失效”“TCP 断开”“Server 退出”与启动时 USB 驱动不匹配。
- 更新阅读指引、工具配置交叉引用、故障表、已知问题、术语表与官方参考链接，保持新增架构说明与本工程实际能力一致。

## 源码测试与历史硬件证据

初版编写时从当前源码运行：

```powershell
cargo test --locked -- --test-threads=1
```

结果：137 项单元测试、1 项 CLI 集成测试通过，0 失败。使用仓库 `.dev` 下的 Rust 环境和本机 MinGW 链接工具。本次为文档修订，未修改产品源码，也没有重新运行或重复计数这些测试。

本次没有连接探针、复位芯片或下载固件，也没有重新执行 Clippy 或发布流程。手册中的 STM32F429 下载、寄存器、断点、运行时读取和长时间运行结论来自已经存在的报告及原始日志，明确标为历史证据。历史报告的源码提交与当前文档基线不同；未将历史请求数算作本次测试数。THA6206 历史说明缺少本机对应原始目录，未将其提升为本次硬件验证结论。

## 编译与结构检查

- 使用 Windows MiKTeX XeLaTeX 连续编译三遍，生成稳定目录和交叉引用。
- 字体：SimSun、SimHei、Microsoft YaHei、Times New Roman、Arial、Consolas；图形由 TikZ 生成，无外部图片依赖。
- 最终日志无编译错误、Overfull、未解析引用或 Missing character。
- 日志剩余 4 处 Underfull 段落间距提示；对应页面已视觉检查，无裁切、遮挡或不可读文本。
- 使用 pypdf/pdfplumber 检查 55 页结构、文字抽取、页边界和替换字符；未发现越过外侧检测边界的字符或 U+FFFD。PDF 含 153 个链接/注释对象。

## PDF 视觉检查

使用 Poppler `pdftoppm -r 85 -png` 渲染全部 55 页，并查看覆盖全部页面的 10 张预览拼图。另按整页尺寸放大检查驱动关系表、DAP/AP 图、CTI/软件联停对照图、Trace 数据流图和故障案例 PDF 第 45–46 页。

本次调整了目录密度、术语表续页表头和来源说明的跨页行为。最后修正 DAP/AP 图中进入 F429 实例的箭头方向，重新编译并渲染全部 55 页，通过像素比较确认仅 PDF 第 19 页变化；该页再次整页视觉检查，其余 54 页与已经检查版本一致。

最终未发现图表/文字重叠、缺字、超出页边界、截断标签、错误箭头遮挡或代码块跨页拆分。目录页码、表格续页表头和页脚正常。

## 清理与复现

最终 `docs` 仅保留 LaTeX、PDF、构建脚本和本记录。临时编译目录（aux/log/toc/out 等）、逐页 PNG 和视觉检查拼图均删除；产品源码没有修改。

在工程根目录执行：

```powershell
./docs/build-usermanual.ps1
```

需要 XeLaTeX、ctex、fontspec、TikZ、listings、xurl、float、needspace 等宏包和上述字体。换到其他平台时可按实际字体修改源文件开头的字体配置。编译脚本不执行硬件测试；更新源码或工具后，需要另行核对手册事实与实板证据。
