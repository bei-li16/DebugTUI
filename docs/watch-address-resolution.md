# Watch 地址解析的只读与上下文证明

2026-10-06，开发分支 `codex/register-debugging`，版本仍为 0.9.3。本批补齐 BUS-006/007 中的地址解析边界，并修正取消标记与多核请求代次；不代表 BUS 整体验收完成。

## 解析规则

`watch_resolve` 仍由 GDB 获得类型、成员路径、`&(expression)` 和 `sizeof(expression)`，不会把表达式的数值当作地址。根表达式和 GDB 返回的 path_expr 都先经过有界语法校验：长度不超过 256 字节、嵌套不超过 16 层；允许成员、数组的只读索引、解引用、只读算术及普通指针类型转换。函数调用、自增/自减、赋值、逗号、调试器变量、C++ new/delete 和不支持的语言语法在求值前拒绝。没有可寻址存储的寄存器、位域、临时结果继续由 GDB 地址查询拒绝。

调用策略使用已有 writer 的 `may-call-functions` 保护：验证当前 on/off 状态，在原值为 on 时关闭，退出时恢复原值。它同时约束重载运算符或类型转换调用目标函数。所有变量对象操作、地址和类型查询都在保护区内执行；对象清理先于策略恢复。清理报错或没有正数 ndeleted、策略恢复失败都会拒绝绑定并进入 FAULT，必须显式重连；原解析错误和清理错误同时保留。

孩子路径不超过 8 层，每层索引小于 4096，并核对当前 child count 和唯一子节点；动态 pretty-printer 节点或缺少稳定身份的节点拒绝。允许的语法并不证明 volatile 或 MMIO 读取没有硬件副作用，也不扩展成任意语言求值器。

## 线程、帧与取消

开始先核对可选 `context`、状态和取消。解析前后分别用 `-thread-info` 和 `-stack-info-frame` 确认选中线程、所有同连接线程都停止、帧级别和帧地址；不切线程、切帧或暂停其他核。只要线程、帧、PC、Context 或已接收的运行/停止/线程通知变化，结果失效。关键查询之间也消费通知并检查取消，避免继续对改变后的上下文发送目标表达式查询；清理和策略恢复仍执行完整。

成功响应保留地址、位宽、大小端、类型和 signed/float，并增加 `context`、`thread`、`frame_address`、`state=STOPPED` 与 `source=gdb_typed_address`。这些证明来自当前 GDB 连接，不证明其物理 CPU 身份。地址仍由原类型求址，不来自 UI 数值猜测。

保留的 Rust Request 克隆调用 `cancel_read()` 现在能取消 `watch_resolve`、`memory_read`、`memory_dump` 和原 GDB `peripheral_read`。Session 与多核 Coordinator 都传递相同的局部取消标记；它不取消控制、写入或整个会话，也不打断正在进行的传输及恢复。Headless JSON 没有新增任意取消标记入口。

## 界面与兼容

Watch 的解析请求携带当前 Context。Watch/Peripherals 监测请求使用实际 worker 的 `register_generation`，UI 自身的协调器 revision 继续用于丢弃迟到界面响应，二者不可混用。UI 在安排后续总线读取前核对返回的 Context、STOPPED 状态、类型来源、线程/帧证明及合法标量位宽；缺少证明的解析结果不会变成可用绑定。

旧 Headless 客户端可以省略 context，成功响应的新字段为增量扩展。内存请求省略 channel 仍表示 GDB；显式提供的 channel 必须是字符串，null、数字、布尔、数组或对象都会报错，不隐式退回 GDB。

## 验证与未完成范围

新增三项表达式单元与两项 UI 单元；七项真实 MI 管道集成覆盖前置拒绝零 MI、原 on/off 策略、静默线程/帧/PC 变化、通知后停止探测、清理/恢复失败、返回路径二次校验、四核 Scope All、实际 worker 取消及错误 channel 类型。取消中途测试用 MI 服务端已收到地址命令的文件信号安排取消，再释放响应；没有用固定睡眠制造竞态。原生 Memory 套件另检查真实 GDB 的成员、浮点、数组孩子和指针转换地址兼容。

最终回归和证据见 [开发进度](registers-development-status.md)，[三项环境 case](../tests/cases/watch-resolution.md) 均 SKIPPED，未上板。软件夹具不能证明目标权限、AP 核归属或物理线程身份。

后续 [绑定与读取来源](bus-read-provenance.md) 已补齐同帧线程/帧/PC 的绑定读取前证明、worker 句柄与 selection_epoch、配置路线指纹、芯片级外设策略和各面板成功 receipt/旧来源展示，并准备三项子集环境驱动。全部 BUS 环境场景、混合服务竞争、新增系统 MMIO 全模块及 Issue #1 整体验收仍未完成。因此不增加 TODO 勾选；其余低 EL、writer、最终验收、升版和非主分支 Release 仍待完成。
