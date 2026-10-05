# R52 新鲜 MMIO 能力 Probe 延后环境用例

REG-405/BUS-006 子集。六项均 **SKIPPED**；本任务不执行上板。

| Case | 独立准备及验收 | 状态 |
| --- | --- | --- |
| MMIO-P01 | 独立核对每核 Debug/GICR 和每 cluster GICD 地址、权限、Device 属性及基线 RAM 一致性；启用 mmio_probe=true，在既有停止点比较 21 项固件 RAM 与 42 个实际请求、完整 owner/route/context/时间。无控制写、解锁、acknowledge、恢复运行或 host 调用固件 | SKIPPED |
| MMIO-P02 | 根据实板 MIDR variant/revision 核对 IIDR；Debug CIDR class9、GIC CIDR classF。错误 CIDR/IIDR 在后续容量地址前停止；访问错误保持 Unknown，不能变成 No；在可隔离的仿真或权限注入环境执行负向用例，不使用猜测物理地址 | SKIPPED |
| MMIO-P03 | EDDFR D28 高字=0、D2C 低字比较器字段，Reserved UNKNOWN 低四位不推断；GICD_TYPER ITLinesNumber=1..30，包含 SGI/PPI 的 INTID64..991 容量边界；独立改变合法配置后未实现条目自动/手工均零 I/O | SKIPPED |
| MMIO-P04 | 非连续核心名称、多 cluster 和可选导出接口；GICR Aff0/ProcessorNumber 与实际 Debug EDDEVAFF0 匹配，绝不由核心名称推导。导出接口错误映射拒绝，不把 board owner 关联当硬件读出的逻辑核心身份 | SKIPPED |
| MMIO-P05 | AP/GDB 两路线、字节序、64 位 GICR 两字非原子；前后值/来源变化，错误核心或 channel 拒绝、无回退；已保存值读失败后保留旧 provenance；单核停止、线程/frame/重连/取消均禁止发布过期事实 | SKIPPED |
| MMIO-P06 | 同 cluster peer 在探测中及发布后运行/停止，共享 GICD Probe 与数据同时失效；私有 Debug/GICR 可保留。同核心普通 TYPER 读取不覆盖独立 Probe 样本。跨 cluster 的 epoch 只影响对应 owner | SKIPPED |

可执行入口默认五阶段 SKIPPED、零目标 I/O：

    node scripts/test-register-mmio-probe-hardware.cjs

具备静态验证环境后，由现有调试环境运行并停在独立固件的 debugtui_mmio_probe_fixture_stop，驱动不运行目标：

    node scripts/test-register-mmio-probe-hardware.cjs --run --binary EXE --project TOML --core CORE --case JSON

先将 tests/fixtures/register-mmio-probe-board.c 集成到专用固件，每核使用独立 slot；调用者独立确认三个映射、供电、权限及静态状态后才传 mapping_verified=1，默认 0 不访问 MMIO。该独立基线要求 normal Hyp、little-endian；这不代表主机 MMIO Probe 的 CPU EL 要求。对象离线编译不证明总线或上板成功。

JSON 示例 tests/fixtures/register-mmio-probe-board.example.json 含虚构地址/软件值。物理执行前删除 software_example，替换 base、owner、route、21 项 expected/reference、实测 expected_facts 和 evidence_source。无已验证映射时保持 SKIPPED。驱动覆盖 P01 和静态容量对照；P02–P06 的真实故障/状态变化需要手工或独立可控环境，软件 worker/MI/AP 测试提供对应自动证据。控制不变沿用 register-mmio.md 十二项用例及原二十四项 raw 驱动。
