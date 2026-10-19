# 优化补验与架构收口记录（2026-10-03）

本轮继续 10 月 2 日的软件架构收口，冻结新增能力，不使用 VM。
本轮软件架构优化及所列可执行补验已完成。五阶段的完整发布验收仍有明确缺口，
不把本机通过外推为正式稳定版或任意设备支持。

## 本轮修复

1. `apps/desktop/src-tauri/src/lib.rs`：真实 Route 不再套用固定的演示延迟、
   丢包与缓冲数值，也不显示“驱动尚未实现/模拟投影”的旧说明。未知指标为 null，
   simulated=false；四条明确的 Demo Route 保留模拟值与标记。沿用现有 DTO 测试。
2. `native_audio_route.rs`：麦克风/扬声器启动超时分别提示 Windows 捕获应用或
   播放输入的前置条件。不改变 60 秒超时和真实媒体就绪要求。
3. `scripts/update_local_debug_bundle.ps1`：完整 schema-v3 安装目录升级运行文件时
   同步清单，记录局部升级来源；回滚恢复清单原始字节。校验器对已安装目录显式
   允许回滚备份，分发包默认校验仍拒绝额外内容。移除/恢复复用相同校验入口。
4. 输入 Route 主动停止先记录本机 Stopping，再通知对端关闭数据通道；
   控制交换失败也释放本机租约。修复正常停止振动时残留 TLS 断流错误的竞态，
   未屏蔽异常断线错误。新版 Host 物理复验通过：正常停止振动/触控板无残留错误，停止振动不影响手柄。
5. `scripts/build_native_source_evidence.py`：可纳入固定哈希的完整 OpenSSL 源码，
   并从固定 Moonlight 归档提取上游构建配方。没有新增生产依赖。

## 已完成软件检查

- Desktop Rust：140 passed、0 failed、10 ignored；严格 Clippy 通过。
- Windows 构建通过；MSVC 的正常“创建库”输出仍产生 linker_messages 提示。
- 文件交付回归在 PowerShell 7 和 Windows PowerShell 5.1 均通过：默认预览、
  升级、已安装目录校验、清单精确回滚、可恢复移除、恢复和篡改拒绝。
- 文档追踪、仓库结构及 diff 空白检查通过。
- OpenSSL 源码 ZIP 与固定哈希一致，当前已安装 APK 的原生库与归档再次核对通过。

完整软件 Gate 的前次结果见 [收口报告](ALPHA_CONSOLIDATION_REPORT_20261002.md)。
本轮针对修改运行上述检查，未重复无关全量测试。

## 已完成有限实机检查

Windows 11 Pro 10.0.26200，本机已安装音频、VHF 触控板、SudoVDA 和 ViGEm
设备均被只读枚举且 device code=0；这本身不是硬件功能通过的证明。
Y700 仍为 TB323FU / Android 16，APK 沿用前次授权安装的完整 0.5.24-alpha.1。

| 路径 | 实际证据 |
| --- | --- |
| 触控板 | 16 次手势更新后 Windows 指针 (629,456) → (1730,456)，触点释放；停止后 Android idle |
| Xbox 360 / IMU | Windows XInput 唯一匹配 leftX=12345；收到 Android 传感器数据；非中性投影包 184 |
| 手柄接收 | acceptedPackets=14,078，rejectedPackets=0，inputPackets=14,058，xinputPackets=13,969 |
| Haptics | XInput 250 ms 低幅振动调用返回 0；Android executedFrames 367 → 379，随后归零 |
| 独立停止 | 停振动后手柄 Active，停手柄后四项主能力仍 Active |
| Display 自动画质 | 2554×1600、60Hz 协商；另完成 10 次出画/Home 停止/返回前台，每次原四条主 Route 仍 Active，服务停止无错误 |

手柄启动时 projectionQueueFull=20，后续检查仍为 20；不声称零丢弃。
显示的首媒体确认不是连续帧率。读取统计覆盖层没有获得文本，不计作呈现速率证据。
手动 1280×720/30Hz 出现实际模式不一致，worker stage=48 后退出；自动模式成功
不能抹去此限制。01:02 后同一旧 Broker 的 30Hz 短时复验成功：只读 GDI 模式
采样记录 SudoVDA 从缓存的 2554×1600/60Hz 切到 1280×720/30Hz，媒体首次呈现确认
为 1，随后正常停止，服务 generation=20、无错误。首次失败尚无确定根因，保留为
偶发限制；本机同时运行 Sunshine，不把共存现象当作已证实的归因。
未更改输出所有权校验，也未强行把 30Hz 降级成 60Hz。

## 已完成：四项主能力两小时持续并发

摄像头、麦克风、扬声器、触控板在同一 Host 118408 下完成两小时运行。Windows 主程序哈希：
`8BD5364197A75409D148452AC88D6C731250396E4A4FF64FA1A5562DEAD22685`。
该构建用于无窗口 Host；最终窗口交付已另行以 tauri/custom-protocol 构建，
包含后续输入停止修复。长测没有在中途更换 Host。

原四条 Route 分别为：

- Microphone：<LAB_ID_03>
- Speaker：<LAB_ID_05>
- Camera：<LAB_ID_10>
- Touchpad：<LAB_ID_12>

每分钟记录 Route、Android 音频帧、最近媒体证据、进程工作集/句柄，并补充系统
CPU 与本项目 TCP/UDP 数量。Windows 麦克风消费端只计数；扬声器使用合成低幅音调。
未保存原始音视频。触控板周期提交空触点；实际手势另有上表证据，不能把空触点
长测称为持续多指操作验收。

原始日志位于 `test-results/y700-soak-*.jsonl` 与同前缀日志。采样器曾因脚本编码
和 Android 显示 Activity 切换时主界面 Binder 为空而重启；原 Host/四 Route 未重启。
保留重复/延迟样本及失败日志，统计应按实际时间，不按 sample 序号假定等间隔。

长测使用原已部署 Broker，哈希
`85D47940ECC1778F9E99084D0952DF5F9E8C217CE0D955026E53DDC95CC6A445`。
新版服务随后部署于 `target/integrated-service-staging/display-alpha-20261003/`；
升级、精确恢复原 ImagePath/账户/启动类型、重新应用新版均已通过服务运行检查。
服务 SHA-256 为 `0CF61D6C1B3FEB8E468CC2ABDC538DE1F883AB53CFC03CE2D7ADD268C9140115`。
证据：`alpha-followup-service-rollback.json`、`alpha-followup-service-final.json`。
这只证明服务生命周期，媒体另行实测。

长测实际时间为 2026-10-02 23:56:20.624 至 2026-10-03 01:56:21.078 +08，
共 7200.454 秒、122 条记录（121 个不同序号）。四条 Route 身份和 epoch 未改变；
麦克风帧 4,437,120 → 350,058,240，扬声器 181,920 → 345,801,120，
摄像头 2,761 → 212,461。Host 工作集 25.01–27.87 MiB、句柄 240–278。
租约采样 70 次，未见过期。部分 CPU/系统采样较晚启动，最长观测间隔 109.379 秒；
不声称无间隙采样、统一丢帧统计或普遍无泄漏。显示 Activity 期间音频观测缺失不计零帧。
结束后逐项停止，剩余 3/2/1 项均保持 Active，最终租约清空，Host 和长测辅助进程退出。
证据：`y700-soak-summary.json`、`y700-soak-resource-summary.json`、
`y700-soak-independent-stop.jsonl`、`y700-soak-cleanup.result`。

最终安装版后续摄像头 worker 异常退出/恢复 10 次通过，每次其他三条主 Route 保持
Active：`y700-followup-camera-crash.jsonl`。四项并发首次启动触控板时发现旧 8 条
控制消息上限不足；Route/租约交换现共用 32 条上限，沿用分发、关联和超时，不增加重试。
修正后四项并发启动通过；现有 140 项测试及严格 Clippy 通过，证据为
`alpha-followup-exchange-tests.log`、`alpha-followup-exchange-clippy.log`、
`y700-exchange-primary-start.log`。扬声器要求辅助进程建立共享环后再打开播放流；
复测中过早打开的测试音源保持零帧，重新打开后正常收帧，此前置条件仍存在。

控制连接故障测试 10 次通过：只阻断本机 Host 到已确认平板控制 TCP 端口，
旧 Route 关闭、旧租约释放、四项能力以新 Route 身份恢复。规则在测试后移除。
这是控制传输故障，不是完整物理断网测试。证据：`y700-followup-control-loss.jsonl`。
Android 应用 force-stop/重开 10 次通过，节点身份和配对保留，旧租约释放，四项能力
重新建立；未清除应用数据或重配对。证据：`y700-followup-android-crash.jsonl`。
新版振动和触控板正常停止无伪断流错误，停止振动后手柄仍 Active；手柄随后正常停止，
触控板重新启动。证据：`y700-final-input-stop.json`。

## 证据入口与未完成项

- `test-results/alpha-followup-desktop-tests.log`、`alpha-followup-clippy.log`
- `test-results/alpha-followup-bundle.log`、`alpha-followup-bundle-ps51.log`
- `test-results/y700-followup-touchpad-cursor.json`
- `test-results/y700-followup-xinput-haptics.json`、`y700-followup-controller.json`
- `test-results/y700-followup-display-auto.json`、`y700-followup-display-home-cycles.jsonl`
- `test-results/y700-followup-display-manual30-retry.json`、`y700-followup-display-modes.jsonl`
- `target/release-evidence/native-source-closure-20261003/`

两小时长测、三类故障恢复、安装版 UI 生命周期、新版显示短测及最终清理均已完成。
本机 S3 可用，但 AC/DC 唤醒计时器均禁用。用户在本轮明确选择暂不执行
锁屏/睡眠，保留未验收项；未改变电源设置。
生产签名、公开分发审查、新机器部署不由本机软件回归替代。

## 本地文件交付与安装版生命周期

真实 72 文件载荷已通过 NSIS 安装、拒绝覆盖、可恢复移除与恢复；旧真实包的
运行文件升级及 EXE/清单精确回滚也通过。证据为
`alpha-followup-real-install.log`、`alpha-followup-real-upgrade.log`、
`alpha-followup-real-removal.log`。这些操作未安装驱动或服务。

输入停止修复版通过关窗继续运行、重开接回同一 Host、UI 中断隔离和菜单退出：
`alpha-followup-final-host-lifecycle.log`。后续控制交换修正更新到同一专用安装目录
`target/alpha-final-app-20261003/` 并通过 72 文件校验。
当前包为 `target/integrated-test/CapyIO-debug-20261002-181423-9028/`，
安装器为 `target/integrated-test/CapyIO-local-20261002-181457-2216-setup.exe`，
SHA-256 `6AE7F7EFCD517BFB82C4BB55953278E52FE9329ACC0264E78A78D22535362F6F`。
当前 Desktop SHA-256 为
`D45930789814340DC1CF211B6A42CD29056D57C3923BA7041F39E3E087233D19`。
安装器已构建；本次修正版通过已有文件升级入口部署，不能把此前安装器的执行记录
说成这个哈希已重新执行安装。证据：`alpha-followup-exchange-upgrade.log`。

包使用 `-UseExistingArtifacts`，记录为 existing-artifacts-source-not-verified；
当前 Desktop、Service/worker 已重建，其他沿用输入不能声称全量同源重建。

## 最终复验与清理

新版服务/修正版 Host 的显示短测通过：自动模式和手动 1280×720/30Hz 各一次，
首次呈现计数分别 10 和 6；各运行 15 秒后 Home 停止，服务无错误，四条主 Route
保持 Active。证据：`y700-final-display.jsonl`。这不消除前文保留的旧版首次模式失败。
修正版 UI 关闭后 Host 78820 不变、四条主 Route 仍 Active：`y700-final-ui-detach.log`。

最终全部主 Route 正常 Stopped、无错误、租约清空；Host、测试音源、捕获及媒体辅助
进程退出。触控板共享恢复为测试前 false，专用 ADB 转发移除，临时故障防火墙规则
不存在。新 Broker 保留 Running，窄范围显示规则随该服务保留；未修改驱动、启动安全
策略、应用数据或配对。证据：`y700-final-cleanup.json`。
文档追踪、manifest、仓库结构及 diff 检查通过，日志为 `alpha-followup-final-*`。

## 收口边界

阶段 1、2、5 的本轮软件交付完成；阶段 3 增补了四主能力两小时并发和三类各 10 次
故障恢复证据，但不是完整矩阵：锁屏/睡眠由用户明确暂缓，物理断网、所有长测指标
同步采样及更广设备覆盖未验收。阶段 4 的本机文件/服务升级回滚完成；新机器系统
组件安装、生产签名、公开分发及全部第三方对应源码/构建可重复性仍未完成。
普通权限摄像头和播放流需在共享环就绪后打开的限制保留。`publicationApproved=false`。
本轮不再扩大功能或重复无关测试；后续以这些明确验收项为边界。
