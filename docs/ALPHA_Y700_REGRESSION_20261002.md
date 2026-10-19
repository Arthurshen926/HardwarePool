# 当前构建 Y700 有限回归（2026-10-02）

## 目标与安装

本轮验证架构重构后的现有能力，不新增功能。Windows 使用用户本机；Android 为
Y700 TB323FU、Android 16，ADB 连接为 `<LAB_IPV4_01>:33457`。控制服务另用
44869 端口，不能把 ADB 端口当作应用端口。证据仅适用于这组设备及当前脏工作树。
Windows 使用 `target/debug/capyio-desktop.exe`，SHA-256 为
`DA560A6CF4BE3979659DDB2FC90EE29F02C4D076FD7CC73EA4980F5DBA8B3449`。

完整 debug APK 由 `scripts/build_integrated_android_debug.ps1` 构建，包含已验证的
Rust JNI 与可选 GameStream 库。构建日志为 `test-results/y700-current-apk-build.log`。
首轮沙箱无法读取 core-lambda-stubs.jar，获准在沙箱外构建后成功。

- 新版本：versionCode 36 / 0.5.24-alpha.1；原版本 35 / 0.5.23-dev。
- APK SHA-256：`AF7EB79F0697E93A59F46DA1AD38BE8C825C9C44F6E1CCF200849A720E4E9D00`。
- 新旧 signer SHA-256 相同：`29307555e525e46a29150bb85d0b73e3fde7270865b215d07b5a95805cc41007`。
- 经用户单独授权后 `adb install -r` 成功，保留 Node ID、应用数据和共享选择。
- 未修改 Android 权限或前台服务声明；未安装/移除 Windows 驱动。

## 配对修复

Windows 与 Android 原配对时间分别为 <redacted-cert-prefix> 和 1790682018564，实际连接
出现 TLS BadSignature/DecryptError。Windows 凭据在原用户环境可解密，未用
跳过证书验证处理失败。用户授权后，通过现有 renewExisting 邀请导出/导入流程
更新这一对设备，新配对时间为 1790954547859，连接与目录交换成功。

只处理主包 dev.capyio.android；未触碰独立 pairinglab 包。新生成的 Windows 与
Android 临时邀请在验证后删除，保留新配对；未清空应用数据或其他设备信任。

## 已执行结果

| 检查 | 结果及限制 |
| --- | --- |
| Android Home/回前台 10 次 | PID 6895、控制监听及共享选择保持；当时没有活动媒体 Route |
| 摄像头 | 管理员 Host 下 Active，最近本地发布帧数 12,781、epoch 1；未作人工画质验收 |
| 麦克风 | Windows 捕获消费者实际收到 575,520 个单声道样本、48 kHz；只计数，不保存录音 |
| 扬声器 | 向明确选定的 CapyIO 渲染端点输入 3 秒合成低幅音调，Android 播放计数 163,200 帧；未作人工听感验收 |
| 三媒体并发 | 摄像头、麦克风、扬声器同时 Active，当前三条 Route error 均为空 |
| 活动媒体时关窗 | UI 53644 正常退出，Host 28268 保持，三条测试 Route 仍 Active |
| 独立停止 | 停摄像头后两条音频 Active；停麦克风后扬声器 Active；最后停止扬声器，活动测试 Route 为零 |
| 控制连接重连 10 次 | 每次显式 disconnect/connect 后 online=true、能力目录 6 项；不等同于物理断网或自动恢复 |

Windows 无媒体的关窗、重开接回同 Host、UI 中断隔离及菜单退出另见收口报告。
本轮活动媒体检查只覆盖正常关窗，没有据此宣称带媒体崩溃恢复通过。

原始证据（忽略目录，不进入仓库）：

- `test-results/y700-background-cycles.json`
- `test-results/y700-capture-count.log`
- `test-results/y700-concurrent-media.json`
- `test-results/y700-active-ui-detach.log`
- `test-results/y700-independent-stop.json`
- `test-results/y700-control-reconnect.json`
- `test-results/y700-cleanup.json`

## 失败与前置条件

普通权限 Host 的摄像头启动失败，错误明确要求管理员运行；沿用已确认的 Alpha
直连契约，以管理员 Host 复测成功。普通权限支持缺口仍然存在。

一条麦克风 Route 在 Windows 未打开捕获消费者时，环缓冲旧计数停在 16,320，
60 秒后因没有新的本地媒体证据超时。打开只计数的捕获消费者后重新申请，计数
由 16,320 增至 25,920，Route 进入 Active。没有修改就绪判定或伪造媒体进展。
这是当前使用前置条件/体验限制，不能写成无条件启动可靠。排查曾怀疑旧 Route
覆盖，但后续证据支持启动超时，未基于早期猜测修改产品代码。

Android 最后查询仍保留早期 speaker Route 的 ENDPOINT_REMOVED 历史失败项，
其 shouldRun=false；不是本轮成功三媒体 Route 仍运行或全部历史错误已清空的证明。
诊断中的旧模拟指标不能用作实际延迟/丢包测量，本记录只采用真实媒体计数。

## 收尾与未覆盖范围

本轮三条媒体均已停止，测试 Host 已退出，ADB tcp:52606 转发与临时邀请/选择器
转储已移除。没有留下本轮运行的摄像头或麦克风采集，也没有保存原始音视频。
保留用户原共享选择、安装后的新版本及获准更新的配对。

这组结果补足音频/摄像头架构路径的有限实机证据。尚未覆盖：每能力及组合两小时
长测、完整续约/撤权矩阵、物理断网、睡眠恢复、带媒体崩溃恢复、触控板及实验
能力的本版完整回归、新机器系统组件部署、生产签名与最终分发材料。
软件架构优化可以收口，完整五阶段发布验收仍不可标记为全部通过。
