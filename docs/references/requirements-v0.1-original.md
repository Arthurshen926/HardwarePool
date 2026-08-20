# 分布式硬件资源共享平台 项目需求文档

> **暂定名称**：OpenHardwarePool / HardwarePool  
> **文档版本**：v0.1  
> **状态**：草案，待评审  
> **适用范围**：项目初期（Android → Windows 试点）及中长期架构规划  
> **文档位置建议**：`docs/REQUIREMENTS.md`

---

## 1. 项目概述

### 1.1 项目背景与问题

现代用户通常同时拥有多台智能设备：手机、平板、笔记本、台式机、嵌入式开发板等。这些设备内部集成了丰富的硬件模块（摄像头、麦克风、扬声器、屏幕、传感器、键盘、触摸板等），但硬件能力被设备边界割裂，无法灵活复用。

现有解决方案多为单点工具：
- 把手机摄像头变成 PC 摄像头：DroidCam、Iriun
- 把手机麦克风变成 PC 麦克风：WO Mic
- 把手机屏幕变成 PC 扩展屏：SpaceDesk
- 把手机变成 PC 键盘/触摸板：AirMouse、Remote Mouse
- 跨设备键鼠共享：Input Leap、Barrier

这些工具彼此独立、协议不同、无法统一管理，用户需要分别安装、配对和配置，无法实现“自由组合硬件能力”。

### 1.2 项目愿景与目标

**愿景**：让任意设备的硬件模块可以像本地设备一样被其他设备按需使用，构建一个开放、跨平台的分布式硬件资源池。

**目标**：
- 将高度集成设备内部的硬件子模块抽象为可共享的“能力单元”
- 支持跨平台、双向、自由组合的硬件共享
- 在 PC 端实现系统级虚拟设备，让第三方应用无感使用远端硬件
- 在移动端提供应用级数据流接口，弥补系统级限制
- 初期聚焦 **Android 手机 → Windows PC** 的硬件共享试点

### 1.3 项目定位与核心价值

- **不是远程桌面**：不整机镜像，而是模块级共享
- **不是 USB/IP**：不共享独立 USB 外设，而是共享集成在系统内部的硬件
- **统一硬件抽象**：提供一套能力描述与协商协议，让硬件模块可插拔
- **开源平台**：允许社区扩展新硬件、新平台

---

## 2. 范围与边界

### 2.1 纳入范围

- 集成在设备内部的硬件模块：摄像头、麦克风、扬声器、屏幕、键盘、触摸板、传感器、执行器等
- 跨平台共享：初期 Android 作为提供方，Windows 作为使用方；后续扩展 Linux、macOS、iOS、嵌入式
- 连接方式：有线（USB RNDIS/ECM）和无线（同一局域网 IP）
- 自由组合：用户可以按需选择共享/使用哪些硬件模块

### 2.2 不纳入核心范围

- 用户可插拔的独立 USB 外设（如 USB 摄像头、USB 声卡、U 盘）
  - 已有成熟 USB/IP 方案，可作为插件兼容，但不作为核心目标
- 广域网/自组网（初期不要求）
- 移动端作为使用方时的系统级虚拟音视频设备（普通 App 无法实现，需 root/系统签名）

### 2.3 平台角色

| 角色 | 说明 |
|---|---|
| **硬件提供方** | 将自己的硬件模块共享出去 |
| **硬件使用方** | 使用远端共享过来的硬件模块 |
| **双向节点** | 同时具备提供和使用能力（手机、PC 等） |
| **轻量节点** | 资源受限设备（ESP32 等），仅共享简单传感器/输出模块 |

---

## 3. 目标用户与应用场景

### 3.1 目标用户

- 拥有多台设备，需要灵活组合硬件的极客、开发者、远程办公者
- 台式机用户，缺少摄像头、麦克风、扬声器、键盘等外设
- 需要将手机传感器/摄像头等能力接入电脑应用的开发者
- 希望参与开源硬件平台的贡献者

### 3.2 典型应用场景

| 场景 | 提供方 | 使用方 | 共享模块 |
|---|---|---|---|
| 视频会议 | 手机 | Windows 台式机 | 摄像头、麦克风、扬声器 |
| 移动办公 | Windows 笔记本 | Android 手机 | 键盘、触摸板 |
| 远程操控 | Windows 笔记本 | Android 手机 | 键盘、触摸板、屏幕 |
| 体感游戏 | 手机 | PC | IMU 传感器 |
| 智能家居 | 手机（红外） | PC | 红外发射器 |
| 多机协同 | Windows 笔记本 | Windows 台式机 | 键盘、鼠标、屏幕、音频 |
| 物联网扩展 | ESP32 | PC/手机 | 传感器、GPIO、显示 |

---

## 4. 功能需求

### 4.1 核心功能列表

- [ ] 设备发现与手动连接（IP 直连）
- [ ] 能力描述与交换
- [ ] 会话协商与模块自由组合
- [ ] 数据传输（音视频/传感器/输入事件/控制）
- [ ] 系统级虚拟设备映射（PC 端）
- [ ] 应用级数据流接口（移动端/传感器）
- [ ] 安全与权限管理
- [ ] 后台稳定性与保活

### 4.2 设备发现与连接

**初期（MVP）**：
- 手动输入 IP 地址建立连接
- 不实现 mDNS/DNS-SD 自动发现

**架构预留**：
- 设计统一的“发现服务”接口，后续可无缝加入 mDNS/Bonjour

**连接方式**：
- 无线：同一局域网 WiFi，基于 IP
- 有线：USB RNDIS/ECM 或 USB 以太网适配器，建立 IP 连接

### 4.3 能力描述与交换

每个设备用结构化 JSON 描述硬件能力，包含：
- 模块标识：如 `camera_back`、`mic_internal`
- 模块类型：`video_source`, `audio_source`, `audio_sink`, `sensor_imu`, `input_keyboard` 等
- 方向：`input` / `output` / `duplex`
- 数据格式：`H.264`, `Opus`, `JSON`, `binary`
- 性能参数：分辨率、采样率、频率范围
- 权限要求：是否需要用户授权（如屏幕采集）
- 挂载级别：`system_device`（可系统级虚拟化）或 `data_stream`（仅应用级数据流）

**示例**：
```json
{
  "capability": {
    "id": "mic_internal",
    "type": "audio_source",
    "direction": "output",
    "formats": ["opus"],
    "sample_rate": 48000,
    "access_level": ["system_device", "data_stream"]
  }
}
```

### 4.4 会话协商与模块自由组合

- 使用方可以选择提供方的单个或多个硬件模块，自由组合
- 会话支持动态增删模块，无需断开整个连接
- 同一设备可以同时向多个对端提供不同模块
- 同一设备可以同时使用来自多个对端的模块
- 控制面：WebSocket + JSON

### 4.5 数据传输

| 模块类型 | 传输方式 | 说明 |
|---|---|---|
| 音视频流 | WebRTC | 低延迟、跨平台、NAT 穿透 |
| 输入事件 | WebRTC DataChannel 或 WebSocket | 低延迟、可靠 |
| 传感器数据 | WebRTC DataChannel / MQTT | 发布/订阅模型 |
| 控制指令 | WebSocket / MQTT | 简单可靠 |

### 4.6 系统级映射

| 平台 | 系统级虚拟设备支持 |
|---|---|
| Windows | ✅ 虚拟摄像头、虚拟声卡、虚拟 HID、虚拟显示器 |
| Linux | ✅ v4l2loopback、PipeWire、uinput |
| macOS | ✅ CoreMediaIO DAL、DriverKit、CGEventPost |
| Android | ❌ 键盘/触摸板可用 Shizuku 注入；音视频系统级不可行 |
| iOS | ❌ 完全封闭，仅应用内使用 |

**初期重点**：Windows 端实现虚拟摄像头、虚拟麦克风、虚拟扬声器。

### 4.7 安全与权限

- 设备配对：首次连接需要确认或输入配对码
- 数据传输加密：TLS / DTLS-SRTP
- 权限管理：提供方可查看和控制哪些硬件模块被远程访问
- 敏感数据（屏幕内容、摄像头）需明确授权
- 开源项目需提供安全审计说明

### 4.8 后台与稳定性

- Android 提供方需要前台服务保活
- 处理系统省电策略、后台限制
- 连接断开后自动恢复或通知
- 视频编码优先使用硬件编码器，降低功耗

---

## 5. 硬件模块需求清单

### 5.1 音视频类

| 模块 | 方向 | 提供方 | 使用方 | 挂载级别 | 参考项目 | 优先级 |
|---|---|---|---|---|---|---|
| 摄像头 | 输入 | Android | Windows | 系统级虚拟摄像头 | DroidCam、OBS Virtual Camera | P0 |
| 麦克风 | 输入 | Android | Windows | 系统级虚拟麦克风 | WO Mic | P0 |
| 扬声器 | 输出 | Android | Windows | 系统级虚拟声卡 | SoundWire | P1 |
| 屏幕 | 输出 | Android | Windows | 应用级/虚拟显示器 | SpaceDesk、scrcpy | P1 |

### 5.2 人机输入类

| 模块 | 方向 | 提供方 | 使用方 | 挂载级别 | 参考项目 | 优先级 |
|---|---|---|---|---|---|---|
| 键盘 | 输出 | Android | Windows | 应用级（SendInput） | AirMouse | P1 |
| 触摸板/触摸屏 | 输出 | Android | Windows | 应用级（SendInput） | AirMouse | P1 |
| 键盘 | 输出 | Windows | Android | Shizuku 全局注入 | scrcpy | P2 |
| 触摸板 | 输出 | Windows | Android | Shizuku 全局注入 | scrcpy | P2 |

### 5.3 传感器类

| 模块 | 方向 | 提供方 | 使用方 | 挂载级别 | 参考项目 | 优先级 |
|---|---|---|---|---|---|---|
| IMU | 输入 | Android | Windows | 应用级数据流 | Sensor Node 等 | P1 |
| 环境光 | 输入 | Android | Windows | 应用级数据流 | 自研 | P2 |
| GPS | 输入 | Android | Windows | 应用级数据流 | 自研 | P2 |
| 气压/温湿度 | 输入 | Android/嵌入式 | Windows | 应用级数据流 | 自研 | P2 |

### 5.4 执行器/输出类

| 模块 | 方向 | 提供方 | 使用方 | 挂载级别 | 参考项目 | 优先级 |
|---|---|---|---|---|---|---|
| 振动马达 | 输出 | Android | Windows | 应用级控制流 | 自研 | P3 |
| 闪光灯 | 输出 | Android | Windows | 应用级控制流 | 自研 | P3 |
| 红外发射器 | 输出 | Android | Windows | 应用级控制流 | 自研 | P3 |

---

## 6. 平台支持与限制

### 6.1 平台支持矩阵

| 平台 | 作为提供方 | 作为使用方（系统级） | 作为使用方（应用级） |
|---|---|---|---|
| Android | ✅ 摄像头/麦克风/屏幕/传感器/输入 | ❌ 音视频系统级；键盘/触摸板可 Shizuku | ✅ 音视频、传感器、数据流 |
| Windows | ✅ 键盘/触摸板/麦克风/摄像头 | ✅ 虚拟设备驱动 | ✅ |
| Linux | ✅ 键盘/鼠标/摄像头/麦克风 | ✅ v4l2loopback/uinput | ✅ |
| macOS | ✅ 键盘/触摸板/摄像头/麦克风 | ✅ CoreMediaIO DAL | ✅ |
| iOS | ✅ 屏幕/摄像头/麦克风（受限） | ❌ | ✅ 应用内 |
| ESP32 | ✅ 传感器/GPIO | ❌ | ✅ MQTT 数据 |

### 6.2 关键限制说明

- **移动端作为使用方时，系统级音视频虚拟化基本不可行**，普通 App 无法注册系统级虚拟摄像头/麦克风/扬声器。
- **Android 作为使用方时，键盘/触摸板可通过 Shizuku 注入全局事件**，无需 root。
- **iOS 最封闭**，只能应用内使用。
- **PC 平台（Windows/Linux/macOS）可完整实现系统级虚拟设备**，是项目的核心优势。

---

## 7. 系统架构

### 7.1 分层架构

```
┌──────────────────────────────────────────────────────┐
│                    控制面（Control Plane）            │
│  - 连接管理（手动 IP / 后续 mDNS）                    │
│  - 能力交换（JSON Schema）                           │
│  - 会话协商（WebSocket + JSON-RPC）                  │
├──────────────────────────────────────────────────────┤
│                    数据面（Data Plane）              │
│  - 音视频流：WebRTC（RTP/RTCP）                      │
│  - 输入事件：WebRTC DataChannel / 自定义TCP/UDP      │
│  - 传感器数据：MQTT / DataChannel / WebSocket        │
│  - 控制指令：MQTT / WebSocket / JSON-RPC             │
├──────────────────────────────────────────────────────┤
│                 平台抽象层（Platform Abstraction）    │
│  - 硬件能力接口：采集/渲染/注入/注册                  │
│  - 平台实现：Android / Windows / Linux / macOS       │
├──────────────────────────────────────────────────────┤
│                 虚拟设备层（Virtual Device Layer）    │
│  - Windows：DirectShow / Media Foundation / Core Audio│
│  - Linux：v4l2loopback / PipeWire / uinput           │
│  - Android：Shizuku / AccessibilityService           │
│  - macOS：CoreMediaIO DAL / DriverKit                │
└──────────────────────────────────────────────────────┘
```

### 7.2 核心模块说明

| 模块 | 职责 |
|---|---|
| **Core** | 能力模型、会话管理、连接管理 |
| **Transport** | 传输插件接口，WebRTC、MQTT、WebSocket 实现 |
| **Platform Adapters** | 各平台采集/注入/渲染适配器 |
| **Virtual Devices** | 虚拟摄像头、虚拟声卡、虚拟 HID 等驱动实现 |
| **UI/CLI** | 用户界面，模块选择、状态显示、配置管理 |

---

## 8. 技术选型与依赖

### 8.1 控制面

- **WebSocket**：信令、能力协商、会话管理
- **JSON**：能力描述、事件格式
- **手动 IP 直连**：初期连接方式，预留 mDNS 接口

### 8.2 数据面

- **WebRTC**：音视频流、DataChannel
  - Android：WebRTC Android SDK 或 `pion/webrtc`
  - Windows：`libwebrtc` 或 `pion/webrtc`
- **MQTT**：传感器数据（可选，初期可用 DataChannel）
- **WebRTC DataChannel**：输入事件、传感器数据

### 8.3 Android 端

- **Kotlin**
- **CameraX / Camera2**：摄像头采集
- **AudioRecord**：麦克风采集
- **MediaProjection**：屏幕采集（需要用户授权）
- **SensorManager**：传感器采集
- **前台服务**：保活

### 8.4 Windows 端

- **C++ / C# / Rust**（建议 C++ 或 Rust 追求性能和驱动集成）
- **WebRTC**：接收音视频流
- **OBS Virtual Camera**（开源）：虚拟摄像头驱动，可复用或参考
- **Core Audio API**：虚拟声卡
- **SendInput**：键盘/鼠标事件注入
- **DirectShow/Media Foundation**：虚拟摄像头实现（可选自研）

### 8.5 借鉴开源项目

| 项目 | 用途 | 语言 |
|---|---|---|
| `pion/webrtc` | WebRTC 实现 | Go |
| `scrcpy` | 屏幕采集与输入注入 | C/Java |
| `OBS Studio` | 虚拟摄像头插件 | C/C++ |
| `Input Leap` / `Barrier` | 跨平台键鼠共享 | C++ |
| `Shizuku` | Android 权限桥 | Kotlin |
| `Mosquitto` | MQTT broker | C |
| `AirMouse` | 浏览器端输入方案参考 | JS |

---

## 9. 分阶段实施路线图

### 阶段 0：项目初始化（1 周）

- [ ] 创建 GitHub 仓库（Apache-2.0）
- [ ] 编写 README、需求文档、架构文档
- [ ] 搭建 monorepo 结构
- [ ] 定义能力描述 Schema 初稿
- [ ] 选择开发语言与工具链

### 阶段 1：MVP 摄像头链路（2-4 周）

**目标**：Android 手机摄像头 → Windows 显示画面

- [ ] Android 端：CameraX 采集 + WebRTC 发送
- [ ] Windows 端：WebRTC 接收 + 窗口显示
- [ ] 控制面：手动 IP + WebSocket 信令
- [ ] 验证延迟、稳定性、编解码

### 阶段 2：系统级虚拟摄像头（2-4 周）

**目标**：手机摄像头出现在 Windows 摄像头列表，第三方应用可用

- [ ] 集成 OBS Virtual Camera 驱动
- [ ] Windows 端将 WebRTC 视频帧写入虚拟摄像头
- [ ] 测试 Zoom、浏览器等应用

### 阶段 3：麦克风与扬声器（3-6 周）

**目标**：手机麦克风变 Windows 虚拟麦克风，电脑声音可输出到手机

- [ ] Android 端：AudioRecord 采集 + Opus 编码 + WebRTC 音频轨
- [ ] Windows 端：虚拟声卡接收播放
- [ ] Android 端：接收 PC 音频流并播放（扬声器共享）
- [ ] 实现音视频同步

### 阶段 4：传感器与输入设备（3-6 周）

**目标**：手机传感器数据、触摸输入共享到 Windows

- [ ] Android 端：SensorManager 采集，DataChannel 传输
- [ ] Windows 端：传感器数据可视化或 API
- [ ] Android 端：触摸事件 → DataChannel → Windows SendInput 注入
- [ ] 手机作为触摸板/键盘使用

### 阶段 5：屏幕共享（可选，3-6 周）

- [ ] Android 端：MediaProjection 采集屏幕
- [ ] Windows 端：视频流显示或虚拟显示器
- [ ] 参考 SpaceDesk、scrcpy

### 阶段 6：架构完善与开源发布（持续）

- [ ] 完善能力描述 Schema
- [ ] 实现多模块自由组合
- [ ] 添加单元测试、集成测试
- [ ] 文档完善：API 文档、开发指南
- [ ] 发布 v0.1 版本

### 阶段 7：扩展平台与双向共享（中长期）

- [ ] Linux 客户端支持
- [ ] macOS 支持
- [ ] Windows → Android 键盘/触摸板注入（Shizuku）
- [ ] 嵌入式设备接入（ESP32）
- [ ] mDNS 自动发现
- [ ] 安全增强（TLS、配对码）

---

## 10. 开源策略与社区

- **许可证**：Apache-2.0
- **仓库结构**：monorepo，模块化
- **文档**：README、CONTRIBUTING、架构文档、API 文档
- **社区**：GitHub Issues、Discord/微信群，标记 `good first issue`
- **推广**：发布到 V2EX、Reddit、开发者论坛，撰写技术博客
- **可持续性**：核心开源，后续可提供云服务、企业版支持

---

## 11. 风险与应对

| 风险 | 应对 |
|---|---|
| 虚拟摄像头/声卡驱动开发复杂 | 复用 OBS Virtual Camera、VB-Cable 等，初期不追求自研 |
| WebRTC 在 Android 后台断连 | 前台服务 + 电池优化白名单 |
| Android 系统版本碎片化 | 设定最低 Android 10，使用兼容 API |
| 跨平台构建复杂 | 使用 CMake/Gradle/GitHub Actions 自动化 |
| 安全漏洞 | 引入 TLS、配对码，进行安全审计 |
| 个人开发者精力有限 | 聚焦 Android → Windows，逐步扩展 |

---

## 12. 待确认事项与决策记录

### 12.1 待确认事项

- [ ] 项目正式名称是否使用 `OpenHardwarePool`？
- [ ] Windows 端开发语言选择：C++ / C# / Rust？
- [ ] 虚拟摄像头是否直接复用 OBS Virtual Camera 驱动，还是自研 DirectShow 插件？
- [ ] 传感器数据传输是否引入 MQTT broker，还是完全使用 WebRTC DataChannel？
- [ ] 初期是否包含屏幕共享模块？
- [ ] 是否需要移动端（Android）作为使用方的输入设备支持（Shizuku）？

### 12.2 决策记录

| 决策 | 结论 | 原因 |
|---|---|---|
| 开源 or 闭源 | 开源（Apache-2.0） | 平台类项目需要生态与信任 |
| 初期平台范围 | Android → Windows | 需求最广，现有参考最多 |
| 连接发现 | 手动 IP，预留 mDNS | 降低初期复杂度 |
| 核心传输 | WebRTC | 低延迟、跨平台、NAT 穿透 |
| 虚拟设备复用 | 优先复用 OBS Virtual Camera | 降低驱动开发成本 |
| 传感器挂载级别 | 应用级数据流 | 系统无标准虚拟传感器接口 |

---

## 13. 附录：现有方案对比

| 工具 | 功能 | 开源 | 与我们差异 |
|---|---|---|---|
| AirMouse | 手机触摸板/键盘 | 是 | 单点输入工具，无虚拟设备 |
| DroidCam | 手机摄像头 | 部分 | 单一摄像头，协议私有 |
| WO Mic | 手机麦克风 | 否 | 单一麦克风 |
| SoundWire | PC 音频→手机 | 否 | 单一音频输出 |
| SpaceDesk | 手机屏幕扩展 | 否 | 单一屏幕共享 |
| scrcpy | 屏幕镜像+反向控制 | 是 | 整机镜像，输入耦合 |
| Input Leap | 多机键鼠共享 | 是 | 仅键鼠，无音视频 |
| 鸿蒙分布式硬件 | 系统级硬件池 | 否 | 系统级，封闭生态 |

**我们的优势**：统一硬件抽象、自由组合、跨平台、开源。

---

> **文档结束**  
> 该文档为项目初始需求稿，后续可根据开发进展和社区反馈持续更新。