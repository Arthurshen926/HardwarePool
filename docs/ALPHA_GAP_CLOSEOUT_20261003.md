# 交付缺口专项收口（2026-10-03）

本轮停止泛化美化，以真实媒体、普通权限摄像头、系统组件交付为主线。
以下为当前事实，不能把接收帧数当作 Windows 应用消费或完整媒体验收。

## 当前机器和部署

Windows：LAB-WINDOWS-HOST；Y700：TB323FU，ADB <LAB_IPV4_01>:37781。
用户确认 CapyIO 本机控制端口为 38317，旧 43193 已拒绝连接。当前配对认证成功，
未重配对、未重装 APK。测试使用普通用户进程，管理员角色检查为 false。

用户单独批准后，本机 CapyIOBroker 已更新为相机分配版本，服务 SHA-256：
`4F2C50E77C17B74091758FD54DF486C1AD7CB17B55E4FE6C2DF8B20CD52AB0D1`。
原配置、目标账号、哈希及结果在 `target/camera-normal-user-20261003/`；
回滚入口 `test-results/closeout-rollback-camera.ps1` 只恢复这项服务变更。
没有装卸驱动或修改启动安全设置。此测试部署不等于更新了当前 Alpha 安装包。

## 实际结果

- 认证连接及资源目录通过。平板上的真实入口是“设置 → 设备连接 → 后台节点连接 →
  后台保持节点在线”；不使用“等待电脑连接”这个不存在的标签。
- 摄像头普通权限接收：36 次采样，发布帧数 46 → 5086，Route Active，正常停止并退出
  本轮 Host。共享内存分配已由服务承担，媒体和相机会话仍属于普通用户进程。
- 系统相机消费首次失败，复测通过：首次 Media Foundation 消费探针首帧报 `0xC00D3704`。
  隔离的占位帧消费通过（1382400 bytes、333333 × 100ns、时间递增），需继续定位
  首次失败原因。随后同一已部署版本重开会话，两次系统消费通过（1382400 bytes，
  时长/时间差均 333333 × 100ns），分别记录首亮度 235 与 16。亮度变化不单独作为
  手机图像内容验证；不能直接归因于远程会话或宣布相机整体完成。
- 音频：已授权，麦克风 Route Active，扬声器未得到消费证据。当前会话的 WinMM
  仅枚举“远程音频”，CPAL 找不到 CapyIO 捕获端点；PnP 仍可见 CapyIO 音频设备
  且状态 OK。消费辅助程序失败，双向端到端验收未通过，不用 Route Active 抹去失败。
- 显示和音频完整回归、锁屏/睡眠没有新增通过记录；锁屏/睡眠仍按用户要求暂缓。

原始证据：`test-results/closeout-camera-summary.json`、`closeout-camera-samples.jsonl`、
`closeout-camera-mf-consumer.log`、`closeout-camera-placeholder-consumer.log`、
`closeout-camera-repeat-consumer-6.log`、`closeout-camera-repeat-consumer-12.log`、
`closeout-camera-frameserver.json`、`closeout-audio-samples.jsonl`、音频 capture/render 的
对应日志。未保存原始音视频。

## 代码及软件检查

ADR 1511 定义仅分配固定共享内存的服务边界：精确 SID ACL、SCM 服务 PID 校验、
有限握手、拒绝已有映射；网络、解码和 MF 会话不移入特权进程。交接修订先初始化
空布局再交给接收器，不重写读者可见的头部。修订产物位于
`target/camera-normal-user-20261003-r2/`，用户于 10 月 4 日授权部署，第二次 UAC 确认后已更新运行；实机结果见下文。

共享内存 5 项测试、camera-host 2 项测试、服务账号配置用例、Desktop 普通权限说明
用例通过；相关库严格 Clippy、前端类型检查及仓库验证通过。编译日志留在
`test-results/camera-allocation-*.log` 和 `camera-reservation-handoff-tests.log`。

服务部署脚本新增显式首次安装、Core 无显示配置安装、精确回滚/新服务移除入口，
并修复停止旧服务后配置失败时未恢复其运行状态的问题。PowerShell 语法检查通过；
首次安装/移除没有在已安装本机上冒充新机验收。应用 NSIS 安装器尚未集成系统组件，
驱动包仍是独立的受控实验部署；签名、目标确认和干净机器验收仍需完成。

## 2026-10-04 控制台会话复验

用户已授权摄像头修订版更新和音频会话切换。无线调试新端口
`<LAB_IPV4_01>:38081` 已连接并核实 TB323FU；CapyIO 控制端口 38317 仍可达。
本机已经处于 local-user 的控制台会话，未执行额外会话切换。

- r2 首次 UAC 调用被取消；用户确认第二次提示后部署成功，服务 Running，SHA-256 为 `00A941DA652EF69F5FC34772924C7D513A05F7301EA380106D25BAD05858DAF8`。结果保存在 r2 的 `deployment-evidence/result.json`。
- 控制台能枚举 CapyIO Speaker with Render Bridge。双向音频 Route 均进入 Active；
  Windows CPAL 捕获在 60 秒记录 2880480 个采样。此计数证明捕获回调运行，
  不单独证明语音内容、音质或延迟。
- 单独停止麦克风后，它为 stopped，扬声器仍为 active；最终 Host 正常退出。
- 第二轮低音量测试音正常完成，用户确认平板听到连续声音；Windows 捕获 `2160000` 个采样（单声道、48000 Hz）。真实出声已确认，尚未验收音质/延迟、语音内容和长时间稳定性。

证据：`test-results/closeout-audio-console-samples.jsonl`、
`closeout-audio-console-capture.log`、`closeout-audio-console-independent-stop.json`。
首轮辅助进程按测试截止清理，未把未完整运行的 render 日志记为正常结束。

r2 相机首轮实机失败保留在 `target/camera-normal-user-20261003-r2/regression-20261004/`。
Android 10:45:40 的退出原因是 APP CRASH(EXCEPTION)，崩溃栈明确为
CameraCaptureService.enterForeground 的 SecurityException（摄像头前台服务启动资格）。
证据 `test-results/closeout-camera-android-crash-20261004.log`。这次尚未走到 Windows
系统消费，不能当成修订后的共享内存交接失败或成功。
已修复服务启动异常未捕获导致整个应用退出的问题：进入现有 ERROR 状态并要求前台重试，
不改变 Android 权限或前台服务声明。回归脚本同时修复断连时停止 Route 失败会跳过 Host
退出的清理问题；本次遗留 Host 已用本地 API 关闭。新 APK 编译与实机验证正在推进。

新版完整 debug APK 已编译并经用户按新哈希单独批准覆盖安装，签名与原包一致：
`B51F3652C49C8C1C1EBA50109ACB9B5DA7A56765EBE3B4E7DC0501C13F30D91B`。
归档 `target/android-ux/20261004-camera-fgs/CapyIO-full-debug.apk` 及 `installation.json`。
首次受限环境构建因 NDK clang Permission denied 失败，真实用户环境构建成功，
包括 verifyIntegratedDebugApk / verifyGameStreamDebugApk；分别保留
`test-results/camera-fgs-build-20261004.log`、`camera-fgs-build-user-20261004.log`。
安装未清除数据、未调整权限或前台服务声明。相机启动限制仍按 Android 系统执行。
音频第二轮证据为 `test-results/closeout-audio-console-repeat-*`，用户明确确认听到连续测试音。


## 2026-10-04 新控制端口 43193 的完成记录

使用已授权部署的 r2 服务和修复后的 Android APK，普通权限实机复验：

- `regression-43193-20261004/result.json`：帧数 1 → 571，系统消费成功，Host 退出。
- `regression-reopen-20261004/result.json`：重开后帧数 1 → 286，消费及清理成功。
- 以上两次仅证明系统取帧；旧探针只取两帧，尚可能是内置占位画面。
- 增强的 `consumer-current-live-probe` 在初始格式/时序检查后最多继续读取 90 帧，
  要求至少 3 帧中部色条签名不同于已知占位图案。它不保存画面，也不声称验证画质或场景。
- `regression-live-content-20261004/result.json`：帧数 1 → 286，普通权限为 true；
  `consumer.log` 记录 `non_placeholder_samples=3`，系统取得非占位内容，清理通过。
- 反例检查：仅启动无平板接收器的相机会话，增强探针按预期失败
  `stage=live_content: only placeholder frames observed`；会话随后退出。
  日志 `test-results/camera-placeholder-negative-20261004.err`。防止把占位画面判为实拍。

上述 regression 目录均在 `target/camera-normal-user-20261003-r2/`。
增强探针 SHA-256 `89C84331207E7C13B4FC412C4C02FBBF1EB87BFF6BF9EC905C10C188DD21A1AD`；
仅更新验证工具，没有再次部署服务或修改驱动。构建和严格 Clippy 通过，日志
`test-results/camera-live-probe-build-20261004.log`、`camera-live-probe-clippy-20261004.log`。

当前可收口的是本机已配置服务、平板前台运行条件下的普通权限摄像头通路及基础音频回归。
仍不覆盖：旧 MF 首次失败的确定根因；Android 后台启动拒绝路径修复后的负向实机测试；
媒体质量/延迟与长测；干净机器完整组件安装；统一 Windows 安装包更新；锁屏/睡眠。
不能把这些短回归记为所有发布门槛完成。
