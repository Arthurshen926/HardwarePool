# 软件架构优化收口报告（2026-10-02）

后续修复与实机补验证据见 [2026-10-03 补验记录](ALPHA_FOLLOWUP_20261003.md)。
本页保留 10 月 2 日时点的结果。

## 结论与范围

本轮以不新增功能、保留 Core 模型为前提，完成本地软件架构与工程基线的整理。
Y700 后续恢复无线调试，获准覆盖安装当前 APK、更新这对设备的配对后，已完成
本机 Windows 与 Y700 的有限实机回归。软件架构优化可以收口；五阶段中的完整
可靠性矩阵和发布验收仍未全部完成，不能据此宣称稳定正式版。

## 已完成的实现

| 边界 | 最终实现 | 主要文件 |
| --- | --- | --- |
| Windows 进程 | UI 仅代理命令；独立 Host 持有 Runtime、连接和 worker；关窗继续、重开接回、菜单退出节点 | node_process.rs、host_commands.rs、background_host.rs、desktop_host/ |
| Route 编排 | 音频、摄像头、显示、手柄及输入 Route 由统一协调入口持有；控制循环不再维护输入状态表 | route_use_coordinator.rs、input_routes.rs、control_worker.rs |
| 公共媒体语义 | 共享状态收件箱、Port 租约、就绪证据和带时间戳的最近媒体观测 | route_status_inbox.rs、route_port_leases.rs、route_readiness.rs |
| Android | 一个系统 Service 组合 Control、Catalog、Consent、Audio、Camera、Display、Controller、Notification 八个职责模块 | platform/android/app/src/main/java/dev/capyio/android/ |
| 工程基线 | 修复目录语义测试、平台告警和 Android 最低版本兼容；建立完整默认软件 Gate | xtask/src/main.rs、scripts/validate_repository.py |
| 功能冻结 | 主能力与实验能力清单、目录标识、可信 LAN/Tailscale 边界 | release/alpha-scope.json、CURRENT_STATUS.md |
| 本地交付 | 文件安装/升级/回滚验证、依赖/许可证收集、原生输入与 APK 字节对应证据 | RELEASE_EVIDENCE.md、scripts/build_release_inventory.py、scripts/build_native_source_evidence.py |

以上 Rust 文件除另列目录外位于 apps/desktop/src-tauri/src/。
授权、lease、epoch 的最终校验仍属于 Runtime，Adapter 数据面和实时回调没有重写。
保留原控制时序、授权等待和续约周期；未加入新的重试层、生产依赖或新测试框架。

## 最终验证

最终 `cargo xtask alpha-check` 输出成功：Rust 汇总（含 Adapter smoke）1116 passed、
0 failed、31 ignored；前端构建与 88 项 Node 测试通过；Android 92 个 Gradle task
成功。完整日志：`test-results/alpha-closeout-final.log`。

当前 Windows debug 构建成功（alpha-closeout-build.log）。最终生命周期检查
（alpha-closeout-host-lifecycle.log）：UI 77200 / Host 101240；关窗保留 Host，
重开 UI 39256 接回 Host 101240；UI 中断隔离通过；菜单退出 UI 返回 0、Host 退出。
两次沙箱内检查未通过（关闭未完成/窗口句柄失效），已保留 first/second-attempt
日志；同一二进制在获准沙箱外运行后通过，未通过修改产品代码或放宽断言掩盖。
该检查没有活动物理 Route。

格式、文档/仓库验证和 diff 空白检查通过。随后完成当前完整 APK 的授权覆盖安装
及有限实机回归，详见 [本次 Y700 验证记录](ALPHA_Y700_REGRESSION_20261002.md)。
三条真实媒体 Route 同时 Active，关闭 UI 后 Host 与 Route 保持；逐条停止隔离通过，
10 次控制连接断开重连通过。该证据不替代两小时长测及物理断网/睡眠恢复。

最终 Gate 首轮暴露既有音频测试时序竞争：UDP 接收可能早于发送线程更新统计。
仅修改 `adapters/audio-share/src/transport.rs` 的该测试，在收包后 join worker 再读
最终统计；未改变生产传输、增加 sleep 或放宽断言。音频 Adapter 14 项测试通过，
1 项实机用例忽略。失败原始日志保留于 alpha-closeout-first-failure.log，修复验证
为 alpha-closeout-audio-fix.log。

已取得的边界证据：

- 手柄与输入归并后的 Desktop 测试：140 passed、0 failed、10 ignored；严格 Clippy 通过。
- Windows 嵌入 UI debug 构建通过；MSVC 正常“创建库”输出仍产生 linker_messages 提示，未屏蔽。
- 独立 Host 正常关窗、UI 中断、重开同 Host PID、菜单退出两进程已通过本机检查。
- 文件 NSIS 安装、覆盖拒绝、升级/回滚、可恢复移除/还原通过；仅使用假载荷。
- 默认 Android Gate 不含可选 GameStream；实验显示另行构建验证通过。
- 原生证据核对 269 个源码文件、2 个集成文件及 APK 内库字节；清单绑定 APK 哈希。
- pre-build 清单：674 个组件/集成记录、901 份许可证文本（含重复）、117 项证据不完整。

原始证据位于忽略目录 test-results/ 和 target/release-evidence/，覆盖范围详见
[交付证据说明](RELEASE_EVIDENCE.md)。旧分步记录保存在
[历史实施记录](reports/ALPHA_CONSOLIDATION_STEPS_20261002.md)，其中中途状态不代表最终实现。

## 五阶段交付状态与延期项

| 阶段 | 软件交付 | 未关闭验收 |
| --- | --- | --- |
| 1 工程基线 | 实施完成，最终检查见上文 | 不外推实机稳定性 |
| 2 功能冻结 | 完成 | 无新增能力 |
| 3 产品可靠性 | 本机生命周期、Y700 三媒体并发与独立停止、控制重连 | 长测、完整并发矩阵、睡眠/物理断网/崩溃恢复 |
| 4 安装发布 | 本地文件交付闭环及证据生成入口 | 系统组件部署、生产签名、实际分发包完整许可证/对应源码材料 |
| 5 架构收口 | 本轮实施完成，音频/摄像头实机回归通过 | 触控板及实验能力的本版完整等价回归；依赖该证据的兼容路径删除 |

兼容路径仍有调用者，不为“清零”而删除。摄像头按用户决定保留管理员直连，普通
权限缺口未关闭。OpenSSL 的头文件/静态库不等于完整对应源码；Windows 显示引擎
仍需单独完成分发材料。未知丢帧率、吞吐、延迟不冒充统一实时指标。

开机启动、整个 Host 重启后的自动拨号、全媒体加密及多节点 Mesh 没有加入本轮。
已按单独授权覆盖安装 APK 并更新这一对设备的配对。未安装或移除驱动，
未使用生产签名、提交、推送或发布；
publicationApproved 保持 false。后续应进入上述验收，不继续无目标地拆分代码。
