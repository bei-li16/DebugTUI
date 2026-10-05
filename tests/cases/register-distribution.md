# REG-107 寄存器目录交付环境 case

以下八项均 **SKIPPED**。主机自动化隔离打包、安装和真实 v0.9.3 包替换另见 [软件自检](../../docs/register-distribution.md)。实际客户操作前备份配置副本，记录原版本／提交／EXE hash、目标包 SHA256、程序与配置根路径、所有客户文件前后 SHA256；测试中的卸载只能指向专用 private prefix。无须连接探针。

| ID | 操作 | 预期 | 状态 |
| --- | --- | --- | --- |
| DIST-H01 | 最终升版构建后用生产脚本生成 npm／直接 EXE／ZIP；对照 SHA256SUMS；在干净路径初始化 | 所有 EXE 版本和 hash 一致；五份 profile 载荷完整，ZIP/template hash 对应源码；用户扩展目录为空，不复制默认模型 | SKIPPED |
| DIST-H02 | 将实际旧客户配置复制到独立 config root；保存自定义设备、tools profile、用户同名模型及嵌套文件；private prefix 安装旧 Release，再升级候选版 | 实际旧包先核对发布方摘要；postinstall 运行；新 EXE/版本/目录真实切换，客户目录和工程逐字节保留，所有来源与报告一致 | SKIPPED |
| DIST-H03 | 使用有中文、空格、UNC／用户重定向配置目录的实际环境；CMD、PowerShell、直接 EXE、ZIP 分别 init/list | 所有入口找到同一个用户配置根；路径无截断；三种内置目录与随包文件相同；不启动 debugger 或硬件 IO | SKIPPED |
| DIST-H04 | 在专用副本放损坏／同名目录／不可读 register override、无效 devices；再测试 `registers` 路径被客户文件占用 | 原数据不覆盖；初始化失败有完整路径/原因，npm hook 返回失败；坏 override 不回退内置；修复后重装可用 | SKIPPED |
| DIST-H05 | 配置根包含现有符号链接／junction 或权限限制，仅在已备份专用目录测试 | 不覆盖／删除链接目标或客户文件；未支持的访问明确失败；记录系统权限、解析路径及错误，禁止把失败当成功初始化 | SKIPPED |
| DIST-H06 | 非连续 core.0/core.2 的项目使用内置、用户目录与显式文件；逐核 list，重开/重装后重复 | 每核 context/source 对应自身 worker；目录选择相同不借其他核身份；仅 metadata 不发 MI/Tcl 或探测 | SKIPPED |
| DIST-H07 | 同包重复安装、卸载、重装，以及直接 EXE／ZIP 替换；客户数据与程序位置分离 | 程序和 launcher 正确替换/移除；用户目录、profile、坏文件与工程 hash 保持；包目录内模板更新不冒充客户文件保留 | SKIPPED |
| DIST-H08 | 全部功能完成、正式 tag/Release 发布后，从公网附件下载并校验，再以真实前一版本升级 | Release commit/tag/资产/安装后版本对应；真实版本变化和客户数据保留；网络／安装错误如实记录；当前软件 fixture 不替代此验收 | SKIPPED |

完成每项后记录 PASS／FAIL、终端截图/文本、安装日志、来源 JSON、前后 manifest/hash 及失败原文。实际终端视觉仍归 REG-208，正式交付／公网升级仍归 REG-505；本任务不包含实际上板测试。
