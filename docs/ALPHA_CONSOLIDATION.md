# Public Alpha 五阶段收口

更新：2026-10-03。本计划按用户附件冻结新增功能；完成计划文件不等于完成五阶段。
当前只接受已有 Windows + Android/Y700 路径的既有证据，不外推跨设备稳定性。

本轮本地软件优化已收口，最终组合 Gate 和 Windows Host 生命周期复核通过，见
[最终报告](ALPHA_CONSOLIDATION_REPORT_20261002.md)。后续进入下表列明的验收与
分发材料工作；不再无目标地扩大软件重构范围。本版已完成有限 Y700 实机回归，见
[实机记录](ALPHA_Y700_REGRESSION_20261002.md)；追加证据见
[10 月 3 日补验](ALPHA_FOLLOWUP_20261003.md)。

| 阶段 | 交付与完成条件 | 当前状态 |
| --- | --- | --- |
| 1 工程基线 | 语义测试、无编译告警、严格 Clippy、全仓测试、前端、Android、仓库验证均通过 | 软件 Gate 已通过；MSVC 链接消息例外见本次报告 |
| 2 功能冻结 | 主能力/实验能力清单；共享目录标识；保留网络限制和发布禁止状态 | 清单与目录标识已落地 |
| 3 产品可靠性 | 下列实机生命周期、长测、并发矩阵保留完整证据 | 部分完成：四项主能力两小时并发、独立停止、带媒体关窗、摄像头 worker/控制连接/Android 应用各 10 次异常恢复；采样限制及剩余矩阵见补验记录 |
| 4 安装与发布 | 新机器安装、升级、回滚、卸载、签名、第三方对应源码/NOTICE/SBOM验收 | SBOM/许可证、完整 OpenSSL 源码材料、本机文件安装闭环和服务升级/回滚已留证；新机系统安装、签名和完整分发材料仍未验收 |
| 5 架构收口 | Headless Host、Service模块化、统一协调与readiness/metrics，至少两种能力迁移并回归 | Android 模块化与 Windows 宿主封装已落地；媒体协调/就绪已接入，独立 Host/IPC 和媒体观测已落地并通过本机生命周期检查；音频/摄像头物理回归通过，其他能力完整回归待验收 |

## 冻结契约

`release/alpha-scope.json` 是本轮范围清单。主流程为配对、连接、目录；主能力为
扬声器、麦克风、摄像头、触控板。Gamepad/Haptics/Display/IMU 标记 Experimental。
保留现有兼容能力，不新增 Hardware Class，不修改 Core 对象族。
“主能力”不代表正式稳定版或任意机器免配置可用。
使用范围限可信 LAN/Tailscale；控制 TLS 不等于所有媒体面应用层加密。

## 阶段 1：可重复软件 Gate

在 Windows 完整开发环境运行 `cargo xtask alpha-check`。该命令依次执行 Rust
format/check/Clippy/test（包含 Desktop）、文档/manifest/Adapter/repository 验证、
前端构建、Android capyioCheck。任一失败或缺少工具即失败，不跳过后宣称通过。
默认 Android Gate 不装入可选 GameStream 显示库；含实验显示的 APK 还须执行
RELEASE_EVIDENCE.md 中的 verifyGameStreamDebugApk，不能将默认 Gate 当作显示装包证据。
此命令不安装 APK、驱动或服务，不使用生产签名密钥，不发布。
命令首次失败后应单独运行后续检查，保留未执行/失败项，不能只记录首个通过项。

## 阶段 3：实机验收协议（部分执行）

先记录 Windows 版本、Android 版本、明确设备标识、两端包 SHA-256、网络类型、
服务/驱动版本和执行账号。不保存私钥、PIN、原始音视频或不必要的个人数据。
以下为完整验收要求；本轮已取得的有限结果单列在实机记录中，不视为整表通过。

| 用例 | 操作 | 必须保留的结果 |
| --- | --- | --- |
| 长测 | 每个主能力运行 2 小时；关键并发组合运行 2 小时 | 每分钟 CPU、工作集、句柄/Socket、帧数、丢弃计数、Route/epoch、租约时间；结束后清理 |
| 生命周期 | 锁屏/解锁、Home/前台、Windows sleep/resume，各 10 次 | 功能各自定义的继续或停止行为；显示退后台停止不误判为失败 |
| 网络 | 断网/恢复 10 次 | 原 Route 不接受旧 epoch 数据；重新连接与授权状态明确；无隐式扩大权限 |
| 崩溃恢复 | 受控关闭指定 worker/Android 进程，再重启，各 10 次 | 无残留设备/端口；仅所属 Route 失败；新运行不继承旧完成消息 |
| 并发隔离 | Speaker+Microphone、Camera+Speaker、Touchpad+Display、Gamepad+Haptics、Audio+Camera+Input | 逐个 Stop；其他 Route 的 epoch/计数继续正常；清理后可重新启动 |
| 撤权/续约 | 跨至少两次 lease renewal；撤销单 Route | 不超越 consent；撤权后停止；不影响另一 Route |

报告必须给出原始数值与趋势，内存增长不能仅凭起止两点宣称泄漏或无泄漏。
任何超时、持续增长或未清理项先定位再复测。实验能力失败不能悄悄降为通过。
自动化 Core 生命周期测试可作为预检，不能代替这些物理验收。

## 阶段 4：交付关闭条件（部分落实）

已增加可重复的构建前 SBOM/许可证收集入口，见 RELEASE_EVIDENCE.md。已在本机
以假载荷验证 NSIS 文件安装、拒绝覆盖、可恢复移除和还原；既有升级/回滚与损坏
拒绝也通过。尚不能据此标记系统组件安装或完整许可证材料通过。

沿用 `LOCAL_DEBUG_UPGRADE.md` 的并排安装与可恢复移除。先验证包完整性与
损坏拒绝、旧版本保护，再在用户指定的 Windows 本机（不用 VM）按授权验收 Service、各 Projection 和
Driver 的安装/重启/卸载。逐项记录精确包、目标与回滚命令；驱动部署、APK 安装、
生产签名和发布仍遵循 AGENTS.md 的单独授权要求。

不得将 `publicationApproved` 自动改为 true。发布前须完成 Windows 签名决策、
Android 签名身份连续性、依赖 SBOM、LICENSE/NOTICE、Vibepollo/Moonlight 对应源码
与本地修改、构建配方及实际二进制清单。依赖清单存在不代表上述材料已齐备。

## 阶段 5：软件架构收口（实施完成，有限物理回归通过）

2026-10-02 用户最初要求先推进软件架构，随后提供 Y700 无线调试并授权覆盖安装
和更新配对。Windows 使用本机，不使用 VM。音频/摄像头已在当前 APK 回归；
阶段 3/4 的完整硬件、安装与发布条件仍不能标记通过。

已接入生产路径：

- Android 原 Service 拆为 Control、Catalog、ResourceConsent、AudioUse、CameraUse、
  DisplayUse、ControllerUse、NodeNotification 八个职责模块；保留原权限与声明。
- Windows DesktopHost 拥有 Runtime、控制连接、平台 worker 和销毁顺序；全部资源
  字段私有，Tauri 通过显式句柄调用宿主方法，不再透传字段。
- RouteUseCoordinator 统一音频、摄像头、显示、手柄的生命周期分派，控制连接循环通过
  它执行启动、停止、授权、状态消息和连接关闭清理。Adapter 保留媒体实现，
  Runtime 保留状态、授权、lease 与 epoch 的最终校验。
- RouteStatusInbox、RoutePortLeases 和 media_lab_endpoints 收拢共享控制实现。
- InputRoutes 将触控板/振动的待授权与活动 Route、续约时钟、反馈关联和数据
  worker 收进统一协调器，控制循环通过统一启停/轮询入口调用。
- RouteReadiness 统一音频/摄像头/显示的就绪证据表达，明确区分 worker 就绪、对端
  实际媒体确认及本机计数进展；仅有 worker 就绪不激活 Route。

- Windows UI 与 Host 是独立进程，同一二进制的 `--node-host` 入口不创建 Tauri
  应用或 WebView。关窗退出 UI，Host 继续运行；重开 UI 接回原 Host。
  “节点 → 退出节点（停止共享）”才关闭节点。正常关窗、UI 进程中断隔离、重开
  连接原 Host PID 和显式退出均已在本机通过（无活动物理 Route）。
- 53 个现有命令经统一 Host 分派，前端命令名/DTO 不变。Local API 使用当前用户/
  登录会话命名管道、精确 SID ACL、服务端程序路径校验、协议版本和 1 MiB 帧上限。
  四个固定 worker 接收控制调用；数据面不经过此 API。没有新增生产依赖。
- RouteMediaSnapshot 统一 Session/Route/epoch、采样时间、worker/媒体就绪和已观察
  帧数；诊断页读取控制 worker 的最近观测，不把未知指标补为零或计算虚假速率。
  每连接最多 32 条、最多 16 个活动连接；超出连接限额明确返回诊断错误。

本轮计划内的宿主进程分离、Android 职责拆分、至少两种媒体协调/就绪/观测整合
已落地。进程内原有会话静态存储现在仅由 Host 使用，不在 UI 复制 Runtime 状态。
开机自动启动、整个 Host 退出后自动恢复连接不属于本轮新增范围；现有 UI 自动
连接策略保留，不能据此声称无 UI 的断网自动恢复已验收。同用户会话的重开需使用
兼容的权限级别；当前管理员摄像头路径的普通权限缺口保留。

升级、回滚或移除前必须通过菜单退出节点，仅关闭窗口不够；新目录的 UI 会拒绝
连接旧目录 Host，避免把新旧版本混用。现有文件脚本仍拒绝处理运行中的目标程序。

阶段 3/4 的完整可靠性矩阵、系统安装和发布验收仍待完成。兼容路径须有物理等价证据才能
删除，不能为了勾选阶段 5 而移除仍在使用的路径。完整五阶段仍不能标为已验收。
