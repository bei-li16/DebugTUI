# R52 GIC 与外部 Debug MMIO 延后环境用例

REG-405/BUS-006 子集。以下十二项均 **SKIPPED**，未上板；软件夹具、离线编译和目录数量不证明真实芯片访问成功。

依据 R52 TRM 100026_0104_01_en §10.2.1、表 10-4/10-35/10-36/12-5，以及 Armv8-R DDI0568A.c G2.1.5、H1.3 DCC 外部读取。准备实际 CPU/修订、独立板级地址图、物理 core/cluster 与 target/endpoint、GICR 控制页、可访问供电状态和已暂停的独立固件。ADS 作为同核、同停止点的原始值参考；不导入其内部资源或据此猜测地址。

| Case | 操作与验收 | 状态 |
| --- | --- | --- |
| MMIO-H01 | 比较外部 MIDR、GICD/GICR IIDR、PIDR/CIDR 与独立板级身份；错误型号/地址/供电返回错误，不能把配置 cpu 当作实测身份 | SKIPPED |
| MMIO-H02 | 两核分别使用独立 Debug/GICR base；GICD 同 cluster 共享、不同 cluster 分离。非连续 core 标签与实际 Aff0/target ID 独立核对 | SKIPPED |
| MMIO-H03 | GICR control 页与 +0x10000 SGI/PPI 页准确；配置绝不按 core 编号、CFGPERIPHBASE 或 CBAR 猜测每核地址 | SKIPPED |
| MMIO-H04 | 64 位 GICR_TYPER 与 IROUTER 用两次 32 位总线字对照完整 raw；字节序与实际通道一致，显示非原子/跨条目非同时采样 | SKIPPED |
| MMIO-H05 | 实测 GICD_TYPER 容量后核对 SPI 最小/最大银行、priority/config/router 边界；Unknown 不自动试读，No 自动/手工均无请求 | SKIPPED |
| MMIO-H06 | EDSCR、断点/观察点与独立 ADS 基线对照；比较器容量以实测 Debug ID 核对，不能由槽位存在或配置事实推断 | SKIPPED |
| MMIO-H07 | 自动刷新不访问 EDPRSR/EDPCSR/TX；显式手工只读一个条目，记录 sticky clear、PC 捕获和 TXfull/MA 传输效果。环境不允许干扰调试通信时保持 SKIPPED | SKIPPED |
| MMIO-H08 | RX 外部读取与 CPU 内部接收区别验证；外部读取不清 RXfull。不用向 RX/TX 写值构造测试条件 | SKIPPED |
| MMIO-H09 | EDITR/EDRCR/OSLAR/LAR 四项 WO 自动/手工均不发送读取；无解锁、指令注入或 writer；GIC pending/active/group/enable 前后不变 | SKIPPED |
| MMIO-H10 | 实际 AP target/endpoint/channel、GDB endpoint、owner、完整请求时间和地址均有来源；通道 core filter 或 owner 缺失零访问，不回退默认地址/其他核/GDB | SKIPPED |
| MMIO-H11 | 取消、切核/线程/frame、下一停止/重连和共享代次变化丢弃旧结果；读失败旧值保留原来源/时间，peer 核状态不变 | SKIPPED |
| MMIO-H12 | EDPFR D20/D24 手册摘要与详细表次序矛盾逐字地址对照；EDDFR D28/D2C 和 EDAA32PFR D60/D64 记录两独立 raw word；不据未经确认的拼接授予能力 | SKIPPED |

可执行入口：node scripts/test-register-mmio-hardware.cjs 默认五阶段 SKIPPED、无目标 I/O。具备环境后执行：

    node scripts/test-register-mmio-hardware.cjs --run --binary EXE --project TOML --core CORE --case JSON

tests/fixtures/register-mmio-board.c 为正常 Hyp、little-endian 的独立固件捕获钩子；每核分配单独 slot，调用者独立确认三个 base、权限和静态中断环境、Device MMIO 属性及基线 RAM 的缓存一致性后才传 mapping_verified=1。未确认保持 0；固件不探测未确认总线、不解锁、改模式或写外设。在 debugtui_mmio_fixture_stop 使用既有调试环境的停止点，不由驱动运行目标函数。此 Hyp 限定用于基线固件，不能据此声称 MMIO 通用读取要求 CPU 为 Hyp。

JSON 模板 tests/fixtures/register-mmio-board.example.json 是 **虚构地址与软件值**。物理执行前删除 software_example，替换 core/cluster、base、路线、reference、expected、MIDR 和独立 evidence_source。固定二十四项为无读取副作用的静态子集；驱动先验证 ready、停止点与新鲜外部 MIDR，再逐项对照独立 RAM，检查控制/配置前后不变，失败仍正常断开。实际 MMIO 不保证原子采样；若运行中的中断源令值不稳定，保留 SKIPPED，不由驱动关闭中断或清除 pending。

该驱动不执行 H05 全容量边界、H06 全比较器、H07/H08 副作用及 H11 真实取消/故障注入；相应手工 case 已定义，软件四 worker/AP 用例覆盖地址隔离、字节序、拒绝和旧值来源。新的物理 MMIO capability Probe、完整 Debug system route 与低 EL ICV 权限仍待开发，REG-405/BUS-006 不勾选。
