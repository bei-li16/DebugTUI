# 本地构建与测试收尾

用户要求：编译、测试完成后清理相关中间文件，避免 C/G 盘被反复构建和分发副本占满。

- 任务结束、验证和打包完成后，执行 `pwsh -NoProfile -File scripts/cleanup-build.ps1 -Apply`。需要继续使用 target 中的程序时，先完成后续验证；release 清理前由脚本核对 bin 中保留的已测试 EXE。
- 分发驱动在 14/14 通过后自动清理安装副本和专用 npm cache；发布打包脚本在验证通过后自动清理本轮 staging/verify。失败夹具只在排查结束后清理，不中断其他正在运行的构建或测试。
- Node `Cases.finish()` 在全部通过、子进程退出后自动去重软件夹具及无损归档大事件日志；硬件、失败、跳过轮次保留。Cargo 回归使用 `pwsh -NoProfile -File scripts/test-and-clean.ps1 test --locked --offline`（可加 `--test <名称>`），通过后只收尾本轮新目录，再清理 target；不扫描其他失败轮次。调试需保留完整产物时设置 `DEBUGTUI_KEEP_TEST_PAYLOADS=1`。
- 历史重复副本用 `scripts/cleanup-test-artifacts.ps1` 预览，确认范围后加 `-Apply`。保留一个原件，删除前校验共享 `.fixture-store` 的无损归档；原路径与 SHA256 在 `manifest.jsonl`。不可删除该存储和旧 gzip 清单；恢复用 `-RestorePath <原文件绝对路径>`，拒绝覆盖、越界、链接及损坏归档。
- 保留源代码、工具链、用户工程配置、Codex/Claude 会话与附件、测试报告及原始上板证据、正式发布附件和对应源码包。不要把将产物移到其他磁盘当成完成清理。
- 清理使用 Cargo/npm 自带命令或已校验路径的原生 PowerShell 操作；拒绝越界路径和链接，不跨 shell 拼接递归删除命令。最终汇报本轮实际释放的空间和保留内容。
