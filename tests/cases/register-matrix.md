# REG-007 能力矩阵环境 case

全部为准备好的延后用例，当前 **SKIPPED，未执行上板**。零目标 I/O 的离线导出可直接验证；有硬件时先取得该核心独立暂停基线并填写 JSON，不能把软件期望复制成板级证明。

| case | 步骤与验收 | 当前状态 |
|---|---|---|
| MATRIX-H01 | 离线导出 R52/R52+，配置路线、位宽、状态条件与显式 owner 一致；无连接/Probe/寄存器读取 | SKIPPED |
| MATRIX-H02 | 选中 core0，再分别指定 core1 和 Scope All；四阶段驱动只读显式 ids，原始值与该核独立基线一致，导出无新增读命令 | SKIPPED |
| MATRIX-H03 | 银行/VFP/Timer/PMU/GIC 分别验证合法当前 EL；对不支持、权限、未使能、未实现和未确定逐项填写期望，完整来源不能互相替代 | SKIPPED |
| MATRIX-H04 | 显式 GICD/GICR/Debug 基址、AP、owner 与独立板级文档一致；导出显示非原子总线路线，新鲜 MMIO Proof 前后的状态条件准确 | SKIPPED |
| MATRIX-H05 | cluster/chip 有效样本后让同 owner peer 改变停止代次；只导出矩阵，共享值 stale/原来源保留，私有值不借用 peer | SKIPPED |
| MATRIX-H06 | RUNNING、换停止点、相关栈帧或 reconnect 后只导出；旧值/Probe 不获得当前支持，当前事实回退到配置声明 | SKIPPED |
| MATRIX-H07 | 隔离测试环境注入协议旧版、短响应或后端恢复失败；用既有 reader 的故障 case 触发，矩阵保持可导出并标未知/失败；不以真实设备人为破坏作默认测试 | SKIPPED |
| MATRIX-H08 | 无 STM 目录、未知 cluster 或缺少核心 target；保留待补/未知及原错误，不自动探测、借用路线或声称硬件不存在 | SKIPPED |

默认不访问目标：

```powershell
node scripts/test-register-matrix-hardware.cjs
```

使用 [JSON 模板](../fixtures/register-matrix-board.example.json) 填写当前核心的独立原始值、位宽与预期状态，替换 software_example/evidence_source。仅使用 side-effect-free 可读条目，最多 128 项；读协议会自行检查当前物理权限，不靠矩阵授权。对其他硬件环境和单核重复同一四阶段；H05/H06/H07 的状态变化用既有专项环境驱动建立，随后导出比对，不在矩阵中增加自动控制动作。

```powershell
node scripts/test-register-matrix-hardware.cjs --run --binary <debugtui.exe> --project <debug.toml> --core <core1> --case <independent-matrix.json>
```

保留 offline.json、matrix.json、完整事件/日志、项目及 case 哈希。矩阵的 hardware_support 仍为 unverified；独立验证结果在外部报告记录，不能把 observed_value 解释为整类/全 EL 支持。软件集成 test 名为 `deferred_register_matrix_driver_runs_actual_binary_and_rejects_bad_independent_values`；带 --software-fixture 的通过报告必须保持 board_tests_executed=false。
