# CapyIO delivery readiness — 2026-09-26

审计基线：`codex/capyio-integrated-alpha`，起始提交 `85d637c`。本轮开发中的
变更须另附构建和部署证据；下表不会把历史设备结果当作当前 APK 已重新通过。

当前状态是统一多功能工程测试版。Android 已将既有能力装入一个 APK，Windows
已有共同界面和后台组件；这不等于所有功能都已接入同一次配对、同一套资源授权
和自动数据端点配置，也不等于普通用户安装完成即可使用。

## 能力验收矩阵

| 能力 | 当前可用实现与入口 | 已有 TB323FU / Y700 证据 | 普通用户交付仍缺什么 |
| --- | --- | --- | --- |
| 触控板 | 统一资源申请、平板确认、租约、独立 mTLS 数据连接、Windows 原生投影和全屏触控区 | 用户重复确认几乎同时四指可以触发 Windows 手势；系统手势关闭后的原生/WebView 计数正常 | 最新停止按钮、五指退出、持续使用续租、双端清理与逆向控制连接的当前构建回归；驱动/权限首次使用流程 |
| 扬声器 | 同一 APK 的 AudioTrack；Windows 音频服务及既有原生 UDP / 兼容入口 | 历史短测 3 秒、144,000 Android 音频帧 | 绑定统一资源 Route、自动配置两端；普通安装下的服务/音频投影部署；听感、时延和长时间测试 |
| 麦克风 | 同一 APK 的 AudioRecord；Windows 原生 LAN 服务，另保留 MicYou 兼容入口 | 历史短测 1,124 包 / 1,079,040 payload bytes，接收器未报告 malformed/wrong-peer/duplicate/discontinuity | 当前平板在普通 Windows 应用中选用虚拟麦克风的完整回归；统一资源授权、端点协商与安装配置 |
| 摄像头 | 同一 APK Camera2/MediaCodec；Windows 托管 ADB reverse、AVC 解码 helper；系统摄像头另有 Broker/COM 源 | 历史 90 AVC access units 解码成 90 NV12 帧，首尾 checksum 不同 | 摆脱 ADB 依赖并绑定统一 Route；系统组件安装/UAC；当前平板在 Windows Camera/OBS 等消费者中的回归 |
| 独立 IMU | 同一 APK SensorManager；与 Controller 共用有界 UDP 发送配置；旧 SensorServer 入口仍保留 | 历史 Controller/IMU 341 包含物理 motion，未宣称零丢包 | 统一认证与资源授权、自动配置；清晰的独立 IMU 接收/统计入口；当前构建六轴/时序/断线回归 |
| 体感手柄 | 同一 APK 触控和 IMU；Windows DSU、DS4/VIIPER/USB-IP 及可选 XInput 边界 | 历史 115 包 non-neutral 状态；这是传输和语义证据 | 当前平板到真实游戏/模拟器的验收、统一控制/数据绑定；可选系统组件部署；远端振动回传未完成 |
| 振动 | 同一 APK 服务拥有的 vibrator Sink 和本地短测试；触控板有本地触感设置 | 历史生命周期与短脉冲命令；触感强弱需人确认 | 游戏/Windows 反馈到平板的独立授权数据 Route；不可把本地触感等同于远程 rumble |
| Display | 同一 APK 接收器；Windows Service Status/Start/Stop；独立连接凭据包 | 曾 HEVC 2640×1216、120 Hz 配置出画，截图记录 456 presented frames；最近重复回归停留 RTSP handshake | 修复重复启动/重连；统一配对和资源 Route；普通配置/安装；对应源码分发与发布构建 gate |

这些历史结果来自 `CAPY_Y700_INTEGRATION_REPORT.md`、
`PEER_CONNECTIVITY_STATUS.md` 和 `TOUCHPAD_CLOSED_LOOP_STATUS.md`。旧手机上的
Windows 系统麦克风/摄像头成功不能作为当前 Y700 的相同结论。帧计数不替代听感、
画面或操作结果；后台 `running` 不替代远端解码呈现。

## 当前实现边界与本轮软件修正

- 两端复用 Vue/TypeScript 导航和资源卡片，但平台能力与配置过程仍不同。
  Windows 的默认 Runtime 仍包含 bootstrap Session 和显式 simulated 的
  audio/projection 目录。目录存在不是本机硬件自动探测已完成。
- `apps/desktop/src-tauri/src/lib.rs` 中统一远端资源的数据 worker 目前只有
  `TouchpadDataWorker`。其他音视频/手柄适配器已有独立可用链路，但尚未绑定到
  这条产品流程。未接入的功能不能仅凭逻辑 Route Active 宣称可用。
- 本轮资源卡片通过 `remote-resource-support.ts` 登记当前已接线的 Source
  Profile，区分“触控板可申请”和“需通过实验功能入口配置”，保留详情导航和
  既有兼容入口。此登记只解释本构建的能力，不替代后端授权和投影检查；未来新增
  真实数据 worker 时应同步更新注册表与验收证据。
- Windows 麦克风 Service 状态尚不提供 native/compatibility 模式字段。本轮
  将其 UI 文案改为中性的“Windows 麦克风后台”，避免统一原生模式也要求打开
  独立 MicYou；直接 MicYou 兼容配置仍保留对应提示。Service 活跃证据称为
  `service_reported_phone_presence`，不误写成经过验证的 TCP 连接。
- Controller 当前 JSON UDP 帧含手工配置的 token；token 校验不是加密数据
  连接。原生音频 UDP 也属于可信实验网络范围。控制 mTLS 不会自动加密这些
  适配器拥有的数据通道。

## 安装与发布阻塞

1. `scripts/build_integrated_test_package.ps1` 当前复制主程序和三个 helper，
   明列 ADB、已部署音频服务/驱动、系统摄像头 UAC、VHF 管理员等前置条件。
   `tauri.conf.json` 已有 NSIS 目标配置，但不代表完整系统组件安装、升级、回滚
   已验收。
   本轮新增 `build_local_delivery_package.ps1`，把统一 APK、内嵌 UI 的 Windows
   程序及三个 helper 放入固定白名单交付包，并验证原 APK 签名与逐文件 SHA-256。
   这关闭了本地双端测试包缺口，不代表正式安装/升级/许可证 gate 已关闭。
2. 首次配对仍使用 `.cpi` 邀请。已配对节点的双向控制连接有证据，但自动发现、
   地址变更、对等发起、占用冲突和全节点启停仍需按实际产品流程完善及测试。
3. `platform/android/app/gamestream.gradle` 仍明确拒绝含 GameStream 的
   `preReleaseBuild`，原因是对应源码分发 gate 未关闭。不能绕过此检查来宣称
   Release 可发布；签名 APK、Windows 安装器及其升级恢复均需独立证据。
4. 当前矩阵不支持“所有功能已验证可交付”的结论。发布前至少应完成当前平板
   每项最终消费者、并发运行、网络故障、独立停止、重连以及最终资源清理测试。

## 建议收尾顺序

1. 闭合当前触控板真实测试：申请/拒绝/停止/退出、持续使用、断线无卡住触点。
2. 未绑定数据的统一资源操作明确拒绝；逐类复用配对、授权、租约和自动端点协商，
   每类接入都以实际接收数据而非逻辑状态作为 Active 的条件。
3. 修复 Display 可重复运行；给出每类普通用户可操作的依赖检测和恢复入口。
4. 对当前 APK/EXE 固定 revision/hash，完成当前设备消费者和多功能故障隔离矩阵。
5. 关闭许可分发 gate、安装/升级/恢复测试后，再产出对应签名和安装包。

## 本轮检查记录

主任务的构建、安装和真实连接复测见
`DELIVERY_VALIDATION_2026_09_26.md`。本轮发现双方配对时间不一致，不能以
历史连接成功证明现有配对正常；真实触控持续使用回归尚未通过。

本审计本身未安装软件、修改权限、采集音视频或操作驱动。当前新增支持判定与
资源卡片渲染的三个测试通过，覆盖真实远端 Source、方向/Profile major/模拟
资源的区分，以及兼容功能仍保留详情入口；共享 UI TypeScript 检查通过。
麦克风文案回归验证服务后台与直接兼容配置的不同提示，实机结果需单独记录。

- `node --test scripts/remote_resource_support.test.mjs scripts/resource_card_render.test.mjs`：3 通过。
- `pnpm --dir apps/desktop typecheck`：通过。
- `cargo test --offline -p capyio-desktop --lib microphone_quick_action::tests`：
  5 通过，1 个需授权硬件的测试保持忽略；执行时其他并行开发文件存在一项
  `RENEW_INTERVAL` 未使用警告，不属于此文案变更。

旧 `INTEGRATION_FEATURE_INVENTORY.md`、`PHYSICAL_ACCEPTANCE_GUIDE.md` 保留了
早期里程碑，包含触控仅 ADB、Android 无拨号等旧描述。使用步骤应结合上述新
触控/对等状态文档，不能把旧段落当作当前全部实现状态。
