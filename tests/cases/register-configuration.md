# REG-102 配置环境验收 case

以下八项均 **SKIPPED**。自动化 `scripts/test-register-configuration.cjs --binary <开发 EXE>` 使用独立配置目录，不连接探针；其十项实际 EXE 检查已纳入 Cargo 集成。以下 case 在真实客户环境/终端中逐项记录，不把软件夹具或目录选择当成板上能力证明。

准备客户配置副本，保留项目、Tools/profile、用户 devices.toml 与用户 register TOML 的 SHA256；记录 DebugTUI 版本/提交、目录来源、CPU 配置与实际目录 CPU。启动 `debugtui --project <副本.toml> --headless --stdio` 后仅发送 `registers_list`、`status`、`select_core`、`quit`；不发送 connect、read/probe、控制或写入。TUI 用 Setup 查看 CPU/目录草稿；Start 会进入真实调试会话，只核对配置时使用取消。

| ID | 操作 | 预期 | 状态 |
| --- | --- | --- | --- |
| CONFIG-H01 | 旧 v1/v2 项目不含 registers；检查已有 GDB 配置与动态 Core 行 | 不强制新目录，旧列表配置保持；只查看配置不启动 GDB | SKIPPED |
| CONFIG-H02 | 分别设置 Tools/root、Tools/profile，再使用显式 `--environment`；显式环境与原 profile 使用不同目录 CPU | 选择顺序与文档一致；显式环境不因原 profile 缺失而报错；Sources 对应真正被选文件 | SKIPPED |
| CONFIG-H03 | 真实 profile 在根和选定 backend 声明 CPU；项目分别覆盖、留空、取消；有芯片关联时再重复 | 项目 > 选定 backend > profile 根；芯片关联只填未选择的默认；空值不被默认覆盖 | SKIPPED |
| CONFIG-H04 | profile/project 使用中文、空格、相对路径、绝对路径及 profile_dir；工程与工具目录不同 | 各目录相对声明文件解析，切核不改变来源；不存在的有效选择报错，不回退 | SKIPPED |
| CONFIG-H05 | 非空 catalogue 与 CPU preset 同时声明；随后清除 catalogue，再同时清除 CPU/catalogue | 文件优先，其后用户 CPU override > 内置，全部清除后回到 GDB；不会把配置 CPU 当作实际目录 CPU | SKIPPED |
| CONFIG-H06 | 用户同名 override 为有效客户文件、损坏文件、目录、不可读或断开的链接，再真正移除该路径 | 有效客户内容优先；损坏/目录/不可读取时必须报错；只有不存在时可选内置；客户文件 SHA256 保持 | SKIPPED |
| CONFIG-H07 | 项目/profile/选定 backend 添加未知 register 设置；nested topology/component 加未知字段或错误类型；组件只覆盖 channel | 错误包含字段/类型信息且在目标操作前报告；有效局部覆盖继承 base/byte order，未知字段不被静默接受 | SKIPPED |
| CONFIG-H08 | 选择非连续核心，如 core.0/core.2；逐核列表，再查看 Setup CPU/GDB/Automatic 草稿、取消与保存 | 每核 context/source 对应真正选择的 worker 与目录；不按列表索引猜系统寄存器 target；只查看/取消无文件和硬件动作，保存仅改项目副本；完整目标不匹配提示仍属 REG-108 | SKIPPED |

涉及不可读文件/链接时仅操作专用测试目录，执行人员选择适合其系统的权限/链接方式。结束核对客户原配置 SHA256 与进程/日志，保留 stderr、目录 list JSON 和 Setup 截图；上板执行不在本任务范围内。
