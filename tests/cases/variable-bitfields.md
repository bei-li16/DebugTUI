# 变量位域延后验收

状态：**writer 未实现；上板 SKIPPED／未执行**。实际本机 GDB 仅验证当前的安全拒绝，不能将该负向测试记为位域写入成功。

基线模型为 32 位声明类型组成的 `{before; unsigned low:5; signed middle:6; unsigned neighbour:7; unsigned reserved:14; after;}`，取初值 `{0x12345678,3,-2,0x55,0x2abc,0x87654321}`。编译时保留 DWARF，先核对真实 bitpos／bitsize、结构体／容器地址与字节序；不能把 GDB `sizeof(field)` 的 4 字节当作字段位宽，也不能将 signed 的 hexadecimal 符号扩展值直接当作 6 位原始值。

| Case | 实现后所需证据／当前行为 | 当前状态 |
|---|---|---|
| BF-H01 | 当前版本 Preview 拒绝 unsigned 5 位和 signed 6 位字段，无 `-var-assign`／内存写回退，全部容器字、邻接哨兵和 calls 不变 | 软件真实 GDB 已验证；上板未执行 |
| BF-H02 | 适配后 low=0/31，middle=-32/31/-1；32、-33 等越界拒绝；读取按真实字段宽度解释，邻接／保留位不变 | writer 待实现，未执行 |
| BF-H03 | Preview 不写；Apply 用新鲜容器，GDB typed assignment 只改变目标字段；软件模型改变邻接位后必须保留新值，服务锁协调另一客户端 | writer 待实现，未执行 |
| BF-H04 | 实际大小端 ABI、跨字节字段及 packed／跨容器情形；无法证明映射、对齐或范围时拒绝，不能套用 host 布局 | writer 待实现，未执行 |
| BF-H05 | const／volatile／MMIO、未知 DWARF、优化／无地址对象、未知字节序／owner／权限拒绝；无 Python 后端不得伪造字段元数据 | writer 待实现，未执行 |
| BF-H06 | core0/core1 私有 RAM、Scope All 当前 owner、shared RAM 全部关联核心暂停；切核／帧／停止代次后草稿过期；peer 或其他字段保持不变 | writer 待实现，未执行 |
| BF-H07 | 发送后错误区分 unknown／accepted／mismatch；无重试、无自动回滚；verified 后才显式恢复并独立读取全部字节 | writer 待实现，未执行 |

软件负向用例来自 `scripts/test-variable-reference-gdb.cjs` 的 `WRITE-T-VAR-BITFIELD-DENIED`。正向 writer 接入前不提供伪装为成功的上板驱动；完整交付仍须补足 BF-H02–07 的实际实现、软件证据和环境就绪后可执行的 case。
