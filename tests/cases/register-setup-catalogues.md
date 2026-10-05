# REG-108 Setup 目录与目标提示环境 case

以下十二项均 **SKIPPED**，没有执行真实终端或上板。主机单元／App 渲染与隔离配置矩阵见 [软件自检](../../docs/register-setup-catalogues.md)。准备客户配置副本，记录项目、profile、devices.toml、用户寄存器目录的 SHA256，以及 DebugTUI 路径、版本、提交、终端尺寸和工具版本。除标明已连接的 case 外，只查看／保存项目副本，不使用 Start 或寄存器读写。

| ID | 操作 | 预期 | 状态 |
| --- | --- | --- | --- |
| SETUP-H01 | CPU registers 逐一高亮 M4、R52、R52+，F1 预览，Esc 取消，然后选择并 Ctrl+S；重新打开项目 | CPU／架构／source 对应真正目录；预览不应用，高亮取消不写文件；保存后只保留选择引用；R52+ 不自动成为已确认身份 | SKIPPED |
| SETUP-H02 | 使用两个非连续核心及带 backend/init/build/download 的 profile；改 CPU 并保存 | Chip、核心名、endpoint、backend、启动命令、下载算法、facts 与 opt-in 保持；原 profile 不改写，每核证据独立 | SKIPPED |
| SETUP-H03 | 用户目录添加有效自定义 preset、内置同名 override；在专用测试副本中分别损坏、替换为目录／不可读，再真正移除 | 用户内容与来源优先；无效 override 显示错误并保留旧草稿／选择器；不能以用户错误为由回退内置；真正不存在时才可用内置 | SKIPPED |
| SETUP-H04 | profile/backend 继承文件与 CPU，项目选择其他 CPU、GDB、Automatic 后各自预览及保存 | CPU 清除继承 catalogue，GDB 显式清空 CPU/catalogue，Automatic 恢复继承；文件优先时配置 CPU 与目录 CPU 分开显示 | SKIPPED |
| SETUP-H05 | CPU picker 的 F2、Register catalogue 的 F2／Enter 分别选中文、空格、绝对路径、UNC／路径副本和相对文件 | F2 浏览 TOML 不替换项目；路径相对声明文件解析；手工输入／浏览同一文件来源一致；保存与重新打开引用正确 | SKIPPED |
| SETUP-H06 | 高亮 preset／文件或输入未提交路径，F1 后尝试 Ctrl+S／F5／点击背景；Esc 依次取消详情、选择器、浏览器、编辑框；选择损坏文件再取消 | 详情只读；背景动作不保存／启动；逐层返回原状态；错误保留浏览器或编辑框与旧草稿／原文件 | SKIPPED |
| SETUP-H07 | 文件实际 CPU 为 M4、配置 CPU 为 R52；再试用户别名 preset | 两个 CPU 值可见，明确文件优先和差异；提醒不改写选择、不判成硬件不存在；别名不等同实际身份 | SKIPPED |
| SETUP-H08 | 用户 Chip CPU 关联与目录不同；随后用户关联置空；未关联型号重复 | 配置不匹配警告带 user／builtin 来源；空值继承已存在的内置关联；无关联不猜 CPU；均标 configuration only | SKIPPED |
| SETUP-H09 | 未连接／无 Probe 时查看 F1、状态详情、各配置核心 | 每核 Observed CPU 为 Unknown；无 GDB／OpenOCD 进程启动或 probe/read 请求，不用 Chip/目录名字代替身份 | SKIPPED |
| SETUP-H10 | 环境允许时，已有停止的实际 R52 核心执行显式 Probe，然后选择 M4／R52+ 目录；切核、运行、复位、重连、切帧再查看 | 仅当前停止核心有有效身份时显示差异警告；其他核心和失效 context 为 Unknown；R52 不被改称 R52+；记录每核原始 MIDR 和 context | SKIPPED |
| SETUP-H11 | 已有实际未适配／不可读 MIDR 时选择 R52+；或打开 Setup 后改 Chip、endpoint、GDB／service／启动配置 | 未适配身份保持 Unknown；旧目标证据不迁移到新草稿；无可选扩展自动试读、使能或 CPU 模式变更 | SKIPPED |
| SETUP-H12 | PowerShell／VS Code 分别在 45×12、80×24、宽屏及高对比度查看中文长说明、来源、所有支持条件；上下／翻页／Home／End／滚轮／Close | 所有信息可滚动访问，未被永久截断，控件无背景误触；记录截图及尺寸；实际视觉仍归 REG-208 | SKIPPED |

环境允许时逐项改为 PASS／FAIL，保存终端截图、错误全文、配置 diff、各核原始身份/context 和前后 SHA256；涉及板的 case 独立记录 probe/read 是否由执行者显式触发。不将软件模拟、目录关联或旧 stop 的值写成实板能力结论。
