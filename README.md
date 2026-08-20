# HardwarePool

HardwarePool 是一个开放、跨平台的分布式硬件能力共享框架。项目把设备内部的硬件模块描述为可发现、可授权、可协商、可组合的“能力”，并在目标操作系统允许时把远端能力投影为本地系统设备。

当前仓库是 **Bootstrap v0.1**：先用 Android 手机的扬声器与麦克风验证统一能力模型、会话生命周期、协议、共享 UI 和 Windows 系统设备投影边界。

## 当前 MVP

```text
Windows application
  -> HardwarePool Speaker (future Windows virtual endpoint)
  -> Windows broker
  -> HardwarePool protocol / audio transport
  -> Android speaker

Android microphone
  -> HardwarePool protocol / audio transport
  -> Windows broker
  -> HardwarePool Microphone (future Windows virtual endpoint)
  -> Windows application
```

麦克风与扬声器是两个独立能力，可以分别映射、分别启停，也可以组成全双工音频会话。

## 本仓库已经包含

- 音频优先的完整需求文档与架构文档；
- Agent 开发规则、安全边界、ADR 与执行计划；
- 纯 Rust 领域 Core：节点、能力、音频格式、会话和投影状态机；
- 纯 Rust Runtime：节点注册、会话操作、事件和 UI 快照；
- 纯 Rust 音频数据面基础件：帧校验、有限乱序缓冲、丢帧统计和时钟漂移估计；
- Protobuf v1 协议定义、Rust 编解码与 Core 转换层；
- 可运行的命令行演示程序；
- Tauri 2 + Vue 3 共用 UI 骨架，浏览器模式内置 Mock Backend；
- Windows 音频驱动、Android、Linux、macOS 平台插槽与边界说明；
- `xtask` 统一命令、GitHub Actions 草案、可直接交给 Agent 的 Backlog 与任务模板。

## 尚未实现

- 真实 Android 音频采集与播放；
- 网络音频数据面、抖动缓冲和时钟漂移修正；
- Windows 虚拟音频驱动及 Broker IPC；
- 设备配对、加密传输和正式安装器。

这些内容按 `docs/ROADMAP.md` 中的 Gate 逐步推进。

## 项目结构

```text
apps/
  hardwarepool-node/       # Headless/CLI 演示节点
  gui/                     # Vue + Tauri 共用 UI
crates/
  hardwarepool-core/       # OS 无关领域模型和状态机
  hardwarepool-audio/      # 音频帧、乱序缓冲和时钟估计
  hardwarepool-runtime/    # OS 无关节点运行时
  hardwarepool-protocol/   # Protobuf 协议和转换
  hardwarepool-testkit/    # 确定性的样例节点与测试夹具
protocol/
  proto/hardwarepool/v1/   # 线上协议定义
drivers/windows-audio/     # Windows 虚拟声卡插槽（尚无驱动代码）
platform/                  # Android/Linux/macOS Adapter 插槽
docs/                      # 需求、架构、安全、测试、ADR
xtask/                     # 统一开发命令
```

## 推荐工具链

- Rust `1.97.1`；
- Node.js `24 LTS`；
- pnpm `11.5.3`；
- Tauri `2.11.2`；
- Windows Driver 工作后续需要匹配的 Visual Studio Build Tools、Windows SDK、WDK 和 WinDbg；
- Android 工作后续需要 JDK、Android SDK、NDK、ADB 和一台真机。

详细要求见 `docs/TOOLCHAIN.md`；首次 Windows 验证见 `docs/FIRST_RUN_WINDOWS.md`；可执行任务见 `docs/BACKLOG.md`；模块入口见 `docs/REPOSITORY_MAP.md`；音频数据面语义见 `docs/DATA_PLANE.md`。

## 快速开始

安装依赖后：

```bash
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo run -p hardwarepool-node -- demo
cargo run -p hardwarepool-node -- audio-frame-demo

corepack enable
pnpm install
pnpm build
pnpm dev
```

浏览器中的 UI 会自动使用本地 Mock Backend；通过 `pnpm tauri dev` 启动时则调用 Rust Runtime。

## 首次接手建议

1. 阅读 `docs/PRODUCT_REQUIREMENTS.md`；
2. 阅读 `docs/ARCHITECTURE.md` 和 `AGENTS.md`；
3. 运行 `cargo xtask doctor`；
4. 完成 `docs/plans/active/0001-bootstrap-validation.md`；
5. 不要在日常 Windows 主系统安装测试驱动。

## 许可证

Apache License 2.0。第三方依赖和未来驱动组件仍需持续维护 `THIRD_PARTY_NOTICES.md` 与许可证审计。
