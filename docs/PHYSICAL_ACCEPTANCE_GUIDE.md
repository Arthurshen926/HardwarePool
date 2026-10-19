# CapyIO 统一版本实机验收清单与使用指南

> 历史验收流程：下文保留旧版本页面和配置描述，不作为当前安装入口。
> 当前包与验收边界见 [本地候选交付](LOCAL_ALPHA_CANDIDATE_20261003.md)，
> 当前使用流程见 [Windows 入门](FIRST_RUN_WINDOWS.md)。

状态：适用于 `codex/capyio-integrated-alpha` 的 0.5.11-dev 集成版本。

界面迭代说明：2026-09-17 的新构建已将导航改为“本机共享、远程资源、设置”。
下文旧的 Quick Actions 对应本机/远程页中的功能详情；Controller、Touchpad
对应资源操作面板；设备与能力中的演示目录移至“设置 → 开发诊断”。
旧测试包和已安装 APK 仍保持旧导航，需使用重新构建的版本。
参见 [共享界面迭代记录](SHARED_UI_RESOURCE_NAVIGATION.md)。

本文面向当前 Lenovo TB323FU（Android 16）与 Windows 实验主机。它是
工程验收指南，不是已经完成的普通用户安装手册。测试过程中不需要启动
历史独立 Android App；手机侧只使用包名 `dev.capyio.android` 的统一 App。

## 1. 本轮验收结论如何判定

每项测试记录为以下四种结果之一：

- **PASS**：真实硬件、真实数据和预期接收端均有证据，停止后资源清理正常。
- **PARTIAL**：生命周期或网络数据已验证，但没有验证最终系统消费者。
- **FAIL**：操作可开始，但数据、投影、清理或用户体验不符合标准。
- **BLOCKED**：缺少已部署的系统组件、权限或当前版本尚未实现该入口。

不能仅凭按钮变成 `active` 判为 PASS。`active` 可能只表示 Adapter 已启动；
必须同时检查下文列出的数据计数、Windows 消费者或手机实际输出。

## 2. 当前软件组成

### Android

- 统一 App：`dev.capyio.android`。
- 默认进入共享 Vue UI 的 **Quick Actions** 页面。
- 同一 App 内包含麦克风、扬声器、Camera2、IMU、触控手柄、触控板、
  振动和显示接收组件。
- `原生诊断` 只用于检查 Android Service 和版本，不是另一套主界面。

### Windows

- 主程序：`capyio-desktop.exe`。
- 本地测试构建由 `scripts/build_integrated_test_package.ps1` 生成到
  `target/integrated-test/<时间戳>/`。
- 必须从完整测试包目录运行，不要单独复制主 EXE。摄像头和触控板入口依赖
  同目录的 `capyio-avc-lab-receiver.exe`、`capyio-camera-session-host.exe`
  和 `capyio-ptp-adb-lab.exe`。
- 后台服务 `CapyIOBroker` 承担系统音频和 Display 生命周期。关闭桌面 UI
  不一定停止已经由服务拥有的功能。

## 3. Windows UI 目前有哪些真实交互

| 页面 | Windows 可操作内容 | 当前边界 |
| --- | --- | --- |
| Quick Actions | 手机麦克风、电脑声音到手机、接收手机触控板、接收手机摄像头、手机作为系统摄像头 | 真实后端；是否可启动取决于已部署组件和配置 |
| Quick Actions / 显示共享 | 启动/停止 Windows Display Sender，刷新服务状态 | `running` 只代表后台已就绪，不代表手机已经出画 |
| Controller | 监听 Android UDP、显示配对 IP/端口/token、实时计数、DSU、DS4 投影、只读预检 | DS4/USB-IP 依赖已部署的实验组件；界面不会安装驱动 |
| 设备与能力 | 本机/远端能力、Node、Adapter、Route 表 | 当前 Tauri 能力目录仍含 fixture；Windows 已有显式发现接收入口，但广播、配对和目录交换尚未接通 |
| 版本与运行检查 | 版本、源码 revision、构建时间、后端检查 | 用于确认没有误开浏览器 Mock |

因此，Windows 端已经有与 Android 对应的交互，但还不是所有功能都达到
“发现设备后点一下自动配对”的普通用户状态。

## 4. 开始前检查

### 4.1 通用检查

- [ ] 手机和 Windows 位于同一可信网络或同一 Tailscale 网络。
- [ ] Android 统一 App 能正常打开，顶部后端显示 `android_node`。
- [ ] Windows UI 顶部后端不是浏览器 Mock。
- [ ] Android 通知、麦克风和相机权限已由系统界面授权。
- [ ] Windows `CapyIOBroker` 为 Running。
- [ ] 未同时启动旧的 Camera、Controller、Touchpad 或 Audio 独立 App。
- [ ] 一个端口只由一条测试链路占用。
- [ ] 记录测试版本、Git revision、手机型号、Android/Windows 版本。

### 4.2 当前实验端口约定

端口可配置，但两端必须严格对应。

| 能力 | Android 本地 | Windows 本地 | 说明 |
| --- | ---: | ---: | --- |
| 手机扬声器 | UDP 40100 | UDP 40101 | Windows 到 Android |
| 手机麦克风 | UDP 40110 | UDP 40111 | Android 到 Windows |
| Controller/IMU | 自动发送端口 | UI 中选择，例如 UDP 40300 | Windows 先监听并生成 token |
| 摄像头 | TCP 38173 | TCP 38173 | 当前桌面端通过 ADB reverse 托管 |
| 触控板 | TCP 61000 | TCP 61000 | 当前桌面端通过 ADB reverse 托管 |
| Display 控制/媒体 | 由连接包定义 | 当前实验配置 7443/7543 | 以私有连接包为准 |

不要把无线调试端口当成 CapyIO 媒体端口。无线调试端口只供 ADB 使用。

## 5. 双端 UI 基础验收

### 操作

1. 分别打开 Android App 和 Windows `capyio-desktop.exe`。
2. 检查两边均有 `Quick Actions`、`设备与能力`、`Controller`；Android 在
   检测到触控能力后还应有 `Touchpad`。
3. 展开 `版本与运行检查`。
4. 在 Android `设备与能力` 中检查七类能力：麦克风、扬声器、摄像头、
   IMU、手柄、振动和触控板。
5. 勾选和取消一个 Source 能力，确认只有共享选择发生变化，没有偷偷启动
   麦克风或相机。

### PASS 标准

- [ ] 两端布局、导航、状态卡和错误呈现风格一致。
- [ ] Android 显示 `android_node`，Windows 显示 Tauri/Windows 后端。
- [ ] Android 能力来自真实硬件目录，`simulated=false`。
- [ ] 勾选共享不会自动申请权限或启动媒体采集。
- [ ] Windows Quick Action 的状态、问题码和按钮与后端状态一致。

### 已知不应误判的内容

- Windows `设备与能力` 页目前不能证明真实远端节点发现成功。
- `Peer Node: No peer` 是当前目录交换未接通，不是手机硬件故障。
- 共享复选框只是本地 publication intent，不等于已经创建跨端 Route。

## 6. 扬声器：Windows 到 Android

### 操作顺序

1. Android `Quick Actions → 本机音频状态`，在连接配置中选择
   `Windows → 本机扬声器`。
2. 填写 Windows 的可信 IPv4、Android 本地端口 40100、Windows 端口
   40101，确认可信网络并保存。
3. Android 启动 `Phone speaker`。
4. Windows Quick Actions 选择要共享的播放设备，或选择已配置的
   `CapyIO Speaker`，再启动电脑声音到手机。
5. 播放连续且容易识别的测试音频 10 秒。
6. 停止 Windows Route，再停止 Android Speaker。

### PASS 标准

- [ ] Android Speaker 状态为 `active`，格式为 48 kHz、双声道、S16LE。
- [ ] `framesProcessed` 连续增加且 10 秒后大于 0。
- [ ] 平板可听到连续声音，无明显周期性卡顿或速度异常。
- [ ] Windows Route 达到 `Active`，没有 queue-full、wrong-peer 或发送错误。
- [ ] Stop 后 Android 帧计数停止增长，通知中的音频所有权消失。

只有“手机有声音”但没有状态/计数证据，记为 PARTIAL。

## 7. 麦克风：Android 到 Windows

### 操作顺序

1. Android 保存 `本机麦克风 → Windows` 配置：本地 40110、Windows
   40111。
2. Windows 先启动 `将手机麦克风用作电脑麦克风`。
3. Android 再启动 `Phone microphone`。
4. 对着平板说话，在 Windows 声音设置、录音机或会议应用中选择 CapyIO
   麦克风并观察输入电平。
5. 录制 10 秒，仅用于本地试听后立即删除。
6. 先停止 Android Microphone，再停止 Windows Route。

### PASS 标准

- [ ] Android 显示 48 kHz、单声道、S16LE，`framesProcessed` 持续增加。
- [ ] Windows Quick Action 达到 `Active`，并确认手机端存在而不是只有进程。
- [ ] 普通 Windows 应用能够选择 CapyIO 麦克风。
- [ ] 说话时输入电平变化，回放可辨认且不是静音、噪声或旧缓存。
- [ ] Stop 后普通应用读不到继续增长的旧音频。

若网络包正常但普通 Windows 应用无法录音，只能记为 PARTIAL。

## 8. 摄像头

### 8.1 应用内接收

1. Windows 先在 Quick Actions 启动 `接收手机摄像头`。
2. Android 刷新 Camera2 来源，选择前/后摄和 AVC 质量。
3. Android 启动 `Phone camera`。
4. 缓慢移动平板并改变画面内容。
5. 停止 Android Camera，再停止 Windows 接收端。

PASS：

- [ ] Windows 接收端进入 Active，而不是只有监听端口。
- [ ] 至少解码 90 个连续 AVC/NV12 帧。
- [ ] 首尾帧校验和不同，画面方向正确。
- [ ] Camera2 状态 Active，系统隐私指示器出现。
- [ ] Stop 后系统不再报告 `dev.capyio.android` 占用摄像头。

### 8.2 Windows 系统摄像头

1. 确认系统摄像头组件已经部署；本测试不负责安装。
2. Windows 启动 `手机作为系统摄像头`，若出现 UAC，核对程序与路径后确认。
3. Android 启动 Camera。
4. 在 Windows Camera、OBS 或会议应用选择 `CapyIO Camera`。

PASS：普通 Windows 应用显示实时手机画面、至少持续 60 秒，停止后虚拟摄像头
会话和 Android Camera 均释放。只有 CapyIO 自身解码成功记为 PARTIAL。

## 9. 触控体感手柄与 IMU

### 操作顺序

1. Windows 打开 `Controller`，选择 UDP 端口并点击 `监听手机`。
2. 记录 Windows UI 显示的 IPv4、端口和临时十六进制 token。
3. Android `Controller` 页面填入这三项并点击 `保存发送目标`。
4. Android Quick Actions 启动 `IMU sensors` 和 `Touch gamepad`。
5. 在 Android 手柄面板依次测试 A/B/X/Y、方向键、肩键、摇杆和扳机；
   同时绕三个轴转动平板。
6. 观察 Windows PC live inspector 和接收计数。

### 基础链路 PASS 标准

- [ ] Windows `Accepted` 连续增加，`Rejected=0`、`Replay=0`。
- [ ] 非中立按键在 Windows 可视化中逐项出现，释放后归零。
- [ ] 加速度和角速度随平板运动变化，不是恒零或固定 fixture。
- [ ] 停止/断网超过 350 ms 后 Windows 自动发送中立态。
- [ ] 重连产生新会话且旧序列不会被接受。

### DSU 验收

1. Windows 选择 `仅手机体感` 或 `手机体感 + 触控按键`，启动 DSU。
2. 在 Cemu/Dolphin 中添加 `127.0.0.1` 对应 DSU 端口。

PASS：订阅者数至少为 1，`Packets` 连续增加，模拟器三个轴方向符合选定
映射，Queue Full 和 Send Errors 均为 0。

### DS4 验收

1. 点击 `只读预检`；只有已部署组件均 Ready 才继续。
2. 保持 Android input connected，选择 DualShock 4 并启动 DS4。
3. 在 Steam 或支持 DS4 的应用中测试按钮、摇杆和体感。

PASS：Windows 枚举一个本次会话拥有的 DS4，`inputPackets` 和
`nonNeutralPackets` 增加，停止后精确移除该设备并回到中立态。不要把可选
XInput companion 作为默认验收路径。

## 10. 独立 IMU

当前统一 APK 的独立 IMU Source 与触控手柄复用同一个有界 UDP 发送配置。
无需启动旧 SensorServer App。

- 只启动 Android `IMU sensors`，不启动 Touch gamepad。
- Windows 保持 Controller listener，观察运动数据和远端 sequence。
- PASS：至少连续收到 300 个样本，三个加速度轴和三个角速度轴均能对
  对应转动做出变化，时间戳单调，停止后不继续伪造数据。

Windows Quick Actions 中的 `Android 实时加速度与角速度` WebSocket 面板是
早期 SensorServer 实验入口，不应作为统一 APK 路径的唯一验收依据。

## 11. 触控板

### 操作顺序

1. Windows Quick Actions 先启动 `接收手机触控板`。
2. 若已有 VHF 实验设备且系统要求 UAC，核对程序后确认。UI 不会安装驱动。
3. Android Quick Actions 启动 `Touchpad surface`。
4. Android 打开 `Touchpad` 页面，测试单指移动、单击、双指移动/滚动和释放。
5. 点击 `释放全部触点`，然后 Stop。

### PASS 标准

- [ ] Android Touchpad 状态 Active，sequence 随触摸增加。
- [ ] Windows 接收端 Active，完成至少一次 `0→1→2→0` 接触变化。
- [ ] Windows 实际光标/滚动产生可预期效果，而不是只收到网络记录。
- [ ] 最多五个触点，离开页面、取消和 Stop 均释放全部触点。
- [ ] 接收器关闭设备并退出，无卡住的 61000 监听。

错误 5 必须显示为权限/投影失败，不能误报成功。

## 12. 振动反馈

1. Android Quick Actions 启动 `Phone vibration`。
2. 感受一次短确认脉冲，然后停止。

PASS：状态进入 Active、平板产生一次短振动、停止后没有持续振动。目前
Windows 尚无完整的远端游戏振动回传控制区；因此本项只能验收 Android 本地
Sink 生命周期，远端 haptics 记为 BLOCKED。

## 13. Display

### 当前正确操作顺序

1. Windows Quick Actions 的显示面板先点击 `启动显示共享`。
2. 等待状态成为 `后台已就绪（尚不代表出画）`。
3. Android 显示面板确认连接文件存在、原生接收组件已加载。
4. Android 点击 `打开显示画面`。
5. 等待认证、RTSP、解码和呈现完成。

### PASS 标准

- [ ] Windows 后台为 Running，且手机端成功通过认证。
- [ ] Android 不能停在 `RTSP handshake` 或反复重连。
- [ ] 手机显示真实 Windows 虚拟显示内容，而非黑屏或状态文字。
- [ ] HEVC 2640x1216、目标 120 Hz 配置被接受，并有非零 presented frames。
- [ ] 鼠标移动或窗口变化能在手机端连续显示。
- [ ] Stop 后 Worker、捕获 helper、虚拟显示租约和端口均释放。

当前 0.5.11-dev 在最近一次回归中停留在 `GameStream: RTSP handshake`，因此
本项当前预期结果为 FAIL，属于交付阻塞问题。Windows 显示 `running` 不能覆盖
手机未出画的事实。

## 14. 多功能并发与故障隔离

在各单项通过后执行：

1. 同时启动 Speaker、Microphone、IMU 和 Gamepad，运行 5 分钟。
2. 再启动 Camera，运行 2 分钟。
3. 单独停止 Camera，确认其余四项继续运行。
4. 单独停止 Speaker，确认 Microphone 不受影响。
5. 关闭网络 10 秒后恢复，观察每条 Route 独立失败或重连。

PASS：没有 App 崩溃、ANR、音频方向串扰或无界内存增长；停止一项不会停止
无关能力；错误归属到具体 Route；最终全部 Stop 后 Android 前台服务和
Windows 子进程均清理。

## 15. 待测记录表

| 项目 | 结果 | 关键证据 | 问题码/备注 |
| --- | --- | --- | --- |
| 双端统一 UI |  | 版本、截图、能力数 |  |
| Windows Quick Actions |  | 可见动作和状态 |  |
| Speaker |  | 帧数、听感、Stop |  |
| Microphone 网络链路 |  | 包数、错误计数 |  |
| Windows 普通应用麦克风 |  | 电平、短录音回放 |  |
| Camera 应用内接收 |  | AVC/NV12 帧数 |  |
| Windows 系统摄像头 |  | Camera/OBS 画面 |  |
| Gamepad 基础链路 |  | accepted/non-neutral |  |
| IMU |  | 样本数、六轴变化 |  |
| DSU |  | subscriber/packet |  |
| DS4 |  | 枚举、动态输入、清理 |  |
| Touchpad |  | 0→1→2→0、光标/滚动 |  |
| Haptics |  | 短脉冲 |  |
| Display |  | presented frames、实际画面 | 当前已知阻塞 |
| 多功能并发 |  | 5 分钟计数与故障隔离 |  |
| 最终清理 |  | 无媒体占用/子进程/临时映射 |  |

## 16. 当前不能作为“全部通过”的条件

在以下项目关闭前，不应称为普通用户可交付版本：

- Display RTSP 握手和重连问题尚未关闭。
- Windows 已有显式网卡上的受限发现接收线程；自动网卡选择、Node 广播、
  Android 发现生命周期、认证和 Capability 目录交换尚未完整接入统一 UI。
- Windows `设备与能力` 页仍有 fixture，不能作为真实硬件检测证据。
- 普通用户安装器、首次配对向导、凭据轮换和自动防火墙配置尚未完成。
- 触控板、DS4、系统摄像头仍依赖预先部署的实验系统组件。
- Windows 普通应用中的麦克风、摄像头和游戏兼容性仍需按本指南重复验收。
