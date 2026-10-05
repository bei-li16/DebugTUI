# 变量位域延后验收

状态：**DWARF typed writer 已有软件证据；上板 SKIPPED／未执行**。真实本机 GCC/GDB、严格大小端／多核故障模型和离线 R52 ELF/DWARF 的验证范围分开记录，均不能作为实板写入成功证明。

独立固件基线见 `tests/fixtures/variable-bitfield-board.cpp`：普通结构体有两侧 32 位哨兵，以及 unsigned 5 位、signed 6 位、邻接 7 位和保留 14 位；另有 packed 跨字节和 unsigned 64／signed 63 位夹具。编译时保留 DWARF，填 case 前独立核对实际 bit offset、bitsize、声明类型／父大小和字节序。GDB `sizeof(field)` 不能作为字段位宽，signed hexadecimal 的符号扩展值也不能直接当字段原始值。本机 packed 大小 6，R52 GNU Arm ABI 大小 5。

| Case | 软件证据与环境就绪时验收 | 上板状态 |
|---|---|---|
| BF-H01 | 缺少 byte order／布局、const／volatile、MMIO／Flash、无实际父 RAM、布局与原始字节不符、超 GDB 写入字或权限不完整时 Preview 拒绝；没有赋值／裸内存回退，原字节不变 | 未执行 |
| BF-H02 | low=0/31、middle=-32/31/-1、unsigned64=0/MAX、signed63=MIN/MAX/-1；越界发送前拒绝；独立父字节和字段值一致，全部邻接／保留位不变 | 未执行 |
| BF-H03 | Preview／Cancel 不写；Apply 前邻接位已经改变时保留新值；服务锁覆盖新鲜读、一次类型赋值和回读，另一客户端不能插入服务事务 | 未执行 |
| BF-H04 | 使用实际大小端 ABI、packed／跨字节字段；GDB 对齐扩大范围的全部额外字节也在声明 RAM 和独立回读范围内；未知／继承／歧义布局拒绝 | 未执行 |
| BF-H05 | 停止代次、线程、帧、ELF、type／layout、parent address、byte order、owner／权限变化使草稿不可写；原调用策略恢复；无 Python ARM GDB 不伪造元数据 | 未执行 |
| BF-H06 | 分别填写 core0、core1、Scope All 当前 owner，以及独立 shared RAM 全部关联核心暂停的工程；核对 peer 和其他字段保持，相关核心运行时拒绝 | 未执行 |
| BF-H07 | 邻接位变化为 mismatch，即使字段匹配；回读失败 accepted，发送后错误 unknown，无重试／自动回滚；只有 verified 才显式恢复，再读取全部原字节 | 未执行 |

```powershell
# 默认只生成 4 skipped 的报告，不访问目标。
node scripts/test-variable-bitfield-hardware.cjs

# 独立固件已暂停在钩子后，复制并核对 case 模板再显式执行。
node scripts/test-variable-bitfield-hardware.cjs --run --project <dedicated-project.toml> --core core0 --fixture-function debug_bitfield_fixture --case <reviewed-bitfield-case.json>
```

模板为 `tests/fixtures/variable-bitfield-board.example.json`。C++ GDB 可能提供 public 伪子节点，模板 `[0,2]` 必须按实际 child/path 核对；C 模型使用 `[2]`。把 `bitfield_cells` 加入 Watch；声明覆盖完整父／可能扩大的写入范围、字节序和当前 owner 的普通 RAM。不得使用未知 MMIO 或任意客户变量代替测试固件。

驱动核对独立 case layout、已暂停的专用钩子／调用帧和全部核心，使用父地址／大小及完整内存字节进行独立字段／邻接验证；取消、一次赋值、no replay 和 verified 后显式恢复均有软件端到端证据。默认不执行任何目标访问，脚本不会运行、暂停或复位 CPU。改变 case 分别执行上述已适配类别；未知／继承布局继续为拒绝，不能记为正向写入。完整文档、其他写入类别与最终交付尚待完成。
