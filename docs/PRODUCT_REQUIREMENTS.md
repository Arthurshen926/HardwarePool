# HardwarePool 产品需求文档

> 文档版本：v0.2-bootstrap  
> 状态：已作为初始仓库基线；实现状态仍为 pre-alpha  
> 初始平台：Android 提供方 → Windows 使用方  
> MVP 能力：手机扬声器、手机麦克风  
> 项目许可证：Apache-2.0

## 1. 背景

现代手机、笔记本、平板和嵌入式设备集成了大量硬件模块，但这些模块通常只能被本机操作系统与应用使用。现有工具往往只解决一个场景，例如手机摄像头、手机麦克风、远程扬声器、扩展屏或跨设备键鼠；它们的身份、协议、权限、会话和设备映射不能组合。

HardwarePool 的目标是把这些模块抽象为独立的“硬件能力”，建立统一的发现、描述、授权、协商、流生命周期和投影模型，并允许平台维护者增加新能力、新传输和新系统适配器。

本需求文档继承原始 v0.1 草案中的模块级共享、跨平台、自由组合、开源平台等目标，但将 MVP 从摄像头优先正式调整为音频优先。原始草案保存在 `docs/references/requirements-v0.1-original.md`。

## 2. 产品愿景

> 建立一个开放、跨平台的设备能力共享框架：远端硬件在系统允许时表现为本地虚拟设备；不能系统级映射时，通过应用级数据流或控制接口提供。

项目不承诺任何平台上的任意硬件都可以完全透明地本地化。统一的是身份、能力语义、权限、会话、流和诊断契约；具体数据面与系统投影由版本化 Profile 和平台 Adapter 实现。

## 3. 核心概念

| 概念 | 定义 |
|---|---|
| Node | 运行 HardwarePool Runtime 的设备或进程实例 |
| Capability | 节点提供的独立硬件或软件能力实例 |
| Profile | 某一类能力的版本化语义，例如 `hardwarepool.audio.capture/1` |
| Session | 两个节点之间的受信任控制关系和生命周期 |
| Binding | 会话中对一个能力的授权、协商和启停状态 |
| Projection | 远端能力在本地的表现形式，例如系统麦克风、系统扬声器或应用流 |
| Transport Binding | 协议语义到具体传输的映射，例如 UDP、QUIC 或 AOO Adapter |
| Bundle | 对多个独立能力的逻辑引用；本身不替代成员能力 |

## 4. MVP 产品定义

### 4.1 设备角色

- Android 手机作为硬件提供方。
- Windows PC 作为硬件使用方。
- 双方运行共享 Core 语义和协议实现，但使用各自的平台 Adapter。

### 4.2 两个独立能力

1. `hardwarepool.audio.render/1`
   - 本地硬件角色：render
   - 数据流角色：consumer
   - 物理目标：Android 内置扬声器
   - Windows 投影：`HardwarePool Speaker`

2. `hardwarepool.audio.capture/1`
   - 本地硬件角色：capture
   - 数据流角色：producer
   - 物理来源：Android 内置麦克风
   - Windows 投影：`HardwarePool Microphone`

二者可以独立映射、独立授权、独立启停，也可以由 `hardwarepool.audio.duplex_bundle/1` 逻辑组合。组合不能把两项权限合并为一个不可拆分授权。

### 4.3 用户价值

- Windows 用户可把系统或单个应用的输出设备切换为手机扬声器。
- Windows 用户可把录音、浏览器、语音输入等应用的输入设备切换为手机麦克风。
- 用户可只开启其中一项，避免不必要的麦克风授权或回声路径。
- 同一套 Core 与 UI 架构为后续摄像头、屏幕、输入、传感器和计算能力保留扩展点。

## 5. 目标用户

- 缺少合适麦克风或扬声器的 Windows 用户；
- 希望复用移动设备硬件的开发者和极客；
- 研究实时音频、系统虚拟设备和跨平台能力编排的开源贡献者；
- 后续希望把内部传感器或执行器接入其他设备的开发者。

## 6. 关键用户流程

### 6.1 连接与配对

1. 用户在 Windows 与 Android 上打开 HardwarePool。
2. MVP 可手动输入局域网 IP；自动发现为后续能力。
3. 首次连接显示双方设备身份与配对码。
4. 用户在提供方设备确认信任关系。
5. Windows 获取 Android 公布的能力列表，但尚未自动使用敏感能力。

### 6.2 使用手机扬声器

1. 用户请求映射 Android `audio.render` 能力。
2. 双方协商音频格式与 QoS。
3. Windows Broker 将投影槽位绑定到远端能力。
4. 用户在 Windows 中选择 `HardwarePool Speaker`。
5. Windows 音频发送到 Android 并由手机扬声器播放。
6. 停止或断线后，Windows 音频服务保持稳定；UI 显示离线状态。

### 6.3 使用手机麦克风

1. 用户请求映射 Android `audio.capture` 能力。
2. Android 显示明确授权和持续前台指示。
3. 双方协商采集模式、格式与音频处理能力。
4. 用户在 Windows 中选择 `HardwarePool Microphone`。
5. Android 麦克风音频送入 Windows 应用。
6. 用户撤销授权后，采集立即停止，Windows 端点输出静音或显式离线状态。

### 6.4 全双工

- 用户可以同时开启两个能力。
- 每条流拥有独立 ID、统计、错误和启停状态。
- AEC/NS/AGC 等能力必须显式协商。
- 不支持 AEC 时，系统不能声称已消除回声。

## 7. 功能需求

### 7.1 节点与身份

- **FR-NODE-001**：每个 Node 必须具有稳定节点 ID、显示名称和平台信息。
- **FR-NODE-002**：会话 ID、消息 ID、能力 ID 和投影 ID 必须彼此区分。
- **FR-NODE-003**：节点必须能够同时声明 provider 与 consumer 角色，为长期双向共享预留。
- **FR-NODE-004**：未知或离线节点不得被自动视为已授权。

### 7.2 能力模型

- **FR-CAP-001**：麦克风与扬声器必须建模为两个独立 Capability。
- **FR-CAP-002**：Capability 必须包含 Profile 名称与主版本号。
- **FR-CAP-003**：Capability 必须分别描述本地角色、数据流角色和支持的投影类型。
- **FR-CAP-004**：Capability 必须描述权限要求、可用状态和格式约束。
- **FR-CAP-005**：一个 Bundle 只能引用成员能力，不得覆盖成员权限、状态或统计。
- **FR-CAP-006**：未来未知能力必须能够作为 opaque 描述保留或被明确拒绝。

### 7.3 会话与绑定

- **FR-SESSION-001**：使用能力必须经历请求、授权、协商、启动和停止阶段。
- **FR-SESSION-002**：扬声器绑定与麦克风绑定必须独立变更。
- **FR-SESSION-003**：停止麦克风不得中断扬声器，反之亦然。
- **FR-SESSION-004**：能力授权必须是带期限、可撤销的 lease。
- **FR-SESSION-005**：远端离线时，会话进入 suspended 或 closed，不得继续把旧包解释为新会话数据。
- **FR-SESSION-006**：协议和 Runtime 必须拒绝非法状态迁移并返回可诊断错误。

### 7.4 音频

- **FR-AUD-001**：MVP 基线格式必须支持 48 kHz、PCM signed 16-bit little-endian。
- **FR-AUD-002**：扬声器基线必须支持双声道；麦克风基线必须支持单声道。
- **FR-AUD-003**：格式协商必须包含采样率、采样格式、通道数、通道布局和帧时长。
- **FR-AUD-004**：每条实时音频流必须携带序列号、单调时间戳与样本索引。
- **FR-AUD-005**：接收端必须能够检测 underrun、overrun、丢包、乱序和时钟漂移。
- **FR-AUD-006**：全双工语音模式必须显式声明 AEC、NS、AGC 的支持与启用状态。
- **FR-AUD-007**：音乐播放和交互语音必须允许选择不同 QoS Profile。

### 7.5 Windows 投影

- **FR-WIN-001**：Windows 最终必须暴露一个系统播放端点 `HardwarePool Speaker`。
- **FR-WIN-002**：Windows 最终必须暴露一个系统录音端点 `HardwarePool Microphone`。
- **FR-WIN-003**：两个端点在远端离线时不得导致 Windows Audio Service 崩溃或无限阻塞。
- **FR-WIN-004**：v0.1 驱动只负责端点、PCM 缓冲、最小 IPC 与状态；网络、协议、编解码和配对必须位于用户态 Broker。
- **FR-WIN-005**：驱动安装、测试签名、Verifier 和调试只能在隔离测试 Windows 中进行。
- **FR-WIN-006**：MVP 允许两个固定投影槽位，不要求根据每台远端设备动态安装新端点。

### 7.6 Android

- **FR-AND-001**：Android 必须通过平台要求的权限流程访问麦克风。
- **FR-AND-002**：麦克风共享期间必须运行可见的前台服务并持续显示使用状态。
- **FR-AND-003**：Android Activity 与长期会话 Runtime 必须解耦，关闭界面不等于静默丢失状态。
- **FR-AND-004**：Android Adapter 必须报告系统实际接受的采样率、通道和缓冲参数，不得只报告请求值。
- **FR-AND-005**：音频路由变化、Audio Focus 变化和权限撤销必须成为显式事件。

### 7.7 控制面与协议

- **FR-PROTO-001**：线上协议必须具有显式主/次版本。
- **FR-PROTO-002**：控制消息必须使用稳定的数字字段号；已删除字段号不得重用。
- **FR-PROTO-003**：协议必须区分控制面消息与实时音频数据帧。
- **FR-PROTO-004**：协议语义不得绑定到单一网络库；传输通过 Adapter 选择。
- **FR-PROTO-005**：不支持的 Profile 主版本、映射或格式必须被明确拒绝。
- **FR-PROTO-006**：MVP 调试输出可以使用 JSON，但高频音频载荷不得使用 JSON。

### 7.8 UI

- **FR-UI-001**：共用 UI 必须显示远端节点、能力、授权和在线状态。
- **FR-UI-002**：用户必须能分别启动和停止麦克风、扬声器投影。
- **FR-UI-003**：UI 必须分别显示两条流的延迟、丢包、缓冲和错误。
- **FR-UI-004**：平台专属操作必须通过明确的 Adapter/Command 暴露，不能散落在 Vue 组件中。
- **FR-UI-005**：在没有原生后端时，UI 必须可以使用确定性的 Mock Backend 进行开发和演示。

### 7.9 诊断

- **FR-DIAG-001**：Runtime 必须产生结构化事件和单调递增序号。
- **FR-DIAG-002**：测试结果必须记录 Git commit、协议版本、OS/设备版本和音频配置。
- **FR-DIAG-003**：日志不得默认记录原始麦克风内容、密钥或长期设备秘密。
- **FR-DIAG-004**：内核驱动不得执行普通文本日志 I/O；只允许受控诊断通道。

## 8. 非功能需求

### 8.1 安全与隐私

- **NFR-SEC-001**：生产连接必须双向认证并加密。
- **NFR-SEC-002**：授权粒度必须为 Capability，而不是整台设备无限授权。
- **NFR-SEC-003**：必须支持立即撤销和租约到期。
- **NFR-SEC-004**：必须防止消息重放、版本降级和会话串线。
- **NFR-SEC-005**：不得把未经验证的网络数据交给内核驱动解析。

### 8.2 稳定性

- **NFR-STAB-001**：网络断开不得使 Windows 音频服务或 Android 系统音频进程崩溃。
- **NFR-STAB-002**：流运行时内存和缓冲水位必须保持有界。
- **NFR-STAB-003**：旧会话残留音频不得在重连后播放。
- **NFR-STAB-004**：每条流连续运行两小时应无崩溃和持续内存增长；发布前需扩展到更长 soak test。

### 8.3 性能

- **NFR-PERF-001**：MVP 在正常局域网中的单向端到端延迟初始目标为中位数不高于约 150 ms；该目标在首轮基线测量后可通过 ADR 调整。
- **NFR-PERF-002**：音频回调不得阻塞、动态分配或等待网络。
- **NFR-PERF-003**：时钟漂移修正必须使长期缓冲水位有界。

### 8.4 可维护性

- **NFR-MAINT-001**：Core 在 Windows、Linux、macOS 上至少可编译和运行单元测试。
- **NFR-MAINT-002**：平台代码不能反向污染 Core 公共模型。
- **NFR-MAINT-003**：每次协议或架构变更必须有 ADR、测试和兼容说明。
- **NFR-MAINT-004**：人类与 Agent 使用相同的统一命令和 Definition of Done。

## 9. MVP 非目标

- 摄像头和视频；
- 音视频同步；
- 广域网中继、NAT 穿透和 Mesh；
- 多人混音和多台手机；
- ASIO、5.1/7.1、空间音频和专业实时演奏；
- Android 捕获其他应用的受保护系统播放内容；
- 电话通话音频捕获；
- Linux/macOS 的系统级音频投影；
- iOS；
- 自动 mDNS 发现；
- NPU/GPU 远程计算；
- 正式驱动签名与商店分发。

以上均可作为后续 Profile 或平台项目，但不得在音频 MVP 稳定前扩大范围。

## 10. MVP 验收条件

### Gate A：共享 Core

- Core 无平台 SDK 依赖；
- 两个能力可独立请求、授权、协商、启动和停止；
- 非法状态迁移有测试；
- 断线后绑定进入可解释状态；
- Core 在至少三个桌面 OS CI 目标上通过格式、Clippy 和测试。

### Gate B：协议

- Node/Capability/AudioFormat 可以 Core ↔ Protobuf 往返；
- Envelope 可以编码/解码；
- 未知主版本和非法枚举被拒绝；
- 字段编号与兼容规则被文档化。

### Gate C：共用 UI

- 浏览器 Mock 模式可显示 Android 节点与两个能力；
- 两个投影可独立启动、停止；
- Tauri Backend 使用与 Mock 相同的 TypeScript 契约；
- UI 显示独立状态和指标。

### Gate D：应用级真机音频

- Windows 用户态测试音可经网络在 Android 扬声器播放；
- Android 麦克风可经网络在 Windows 保存为 WAV；
- 长时间运行无累计缓冲漂移；
- Android 权限、锁屏、后台与路由测试通过。

### Gate E：Windows 系统端点

- 测试 VM 中出现两个端点；
- Windows 普通应用能够选择它们；
- 两条流独立；
- 断线、重连、音频服务重启和系统重启不会造成驱动违规；
- Driver Verifier 只针对自研驱动且无违规。

## 11. 长期方向

MVP 完成后按独立 Profile 增加：摄像头、屏幕采集/显示、HID 输入、传感器、执行器和远程推理。长期节点可以同时作为提供方与使用方；不同平台可以选择系统投影或应用数据流，而不改变 Core 生命周期。

## 12. 开放问题

- 正式项目名称与包命名；
- 首个实际音频 Transport：自研参考 UDP/PCM、AOO Adapter、QUIC/RTP 或其他方案；
- Windows Driver 与 Broker 的最终 IPC 机制；
- 音频端点属性、默认格式与共享/独占模式边界；
- 生产配对协议与密钥存储；
- Android CPAL、Oboe 或专属 Kotlin/AAudio Adapter 的取舍；
- 公开发布前的驱动签名和证书成本。

这些问题必须通过小型 Spike、测量和 ADR 解决，不应由单次 Agent Prompt 隐式决定。
