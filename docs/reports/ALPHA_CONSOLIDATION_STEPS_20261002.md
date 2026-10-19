# Alpha 收口验证记录（2026-10-02）

## 最新收口：输入 Route 归入统一协调器

`input_routes.rs` 接管原控制循环中的待授权 Route、已启动 Route、数据 worker、
续约时钟与手柄反馈关联；由 RouteUseCoordinator 持有，生命周期仍属于连接。
`control_worker.rs` 不再维护这三张表，所有资源启动/停止经过统一分派。
保持媒体轮询、反馈清理、手柄轮询、反馈请求、端点广告和输入轮询的原顺序。
未新增功能、生产依赖、防御层或测试套件；不改变超时、授权或数据面。

严格 Desktop Clippy 与既有 Desktop 测试通过：140 passed、0 failed、10 ignored。
完整仓库结构验证通过。日志：test-results/input-routes-check.log、
input-routes-clippy.log、input-routes-tests.log。此前“触控板/振动仍留在控制循环”
的描述为历史步骤，已由本节和 CURRENT_ARCHITECTURE.md 取代。

## 补充收口：手柄统一分派与控制 worker 边界

重新对照附件后，补齐此前仍独立放在控制循环中的手柄协调器所有权。
`route_use_coordinator.rs` 现在持有手柄协调器与 GamepadLab 引用，集中手柄启动、
停止、授权、状态分派、反馈请求标志和断连清理。保留原有状态处理与 tick 顺序，
不改变手柄数据面、授权或租约。触控板/振动仍保留既有连接级 Route 管理。

`control_worker.rs` 从 lib.rs 移出完整 ControlLinkController 实现，集中连接调度、
命令队列和消息循环；lib.rs 不再内嵌约 700 行连接 worker。没有新功能、新依赖、
重试层或额外测试套件。ADR 1509 与当前架构/状态导航已同步。

验证：Desktop check、严格 Clippy 通过；手柄整合后的既有 Desktop 测试
140 passed、0 failed、10 ignored；模块移动后的 check/Clippy 和完整仓库结构验证
通过。嵌入 UI 的 Windows debug EXE 已重新构建成功；仍有已记录的 MSVC 创建库
链接提示，未屏蔽。日志为 test-results/route-controller-*.log、control-worker-*.log。
实机回归与系统部署仍按此前决定延期，不因本次行为等价整理而标记通过。

## 本轮最终状态

用户授权范围内可在当前本机完成的软件架构实施已收口：独立 Host/Local API、
Android 八模块拆分、媒体协调/readiness/观测、范围冻结及本地交付证据入口均已落地。
没有新增产品能力或生产依赖。保留仍在使用的兼容路径，删除须等待物理等价证据。
以下按阶段区分实施与验收，历史步骤中“架构待迁移”的描述不再代表当前状态。

| 阶段 | 本轮结果 | 尚待外部条件 |
| --- | --- | --- |
| 1 | 完整默认软件 Gate 通过 | 已记录链接提示；不等于实机稳定 |
| 2 | 主能力/实验能力冻结落地 | 无新增范围 |
| 3 | 实机矩阵已明确；Host 本机生命周期通过 | Y700 缺席，长测/并发/恢复延期 |
| 4 | SBOM/许可证收集、原生证据、文件安装/升级/回滚闭环 | 系统部署、签名与完整分发材料未验收 |
| 5 | 本地软件架构实施完成 | 物理回归及兼容路径删除延期 |

补充验证了实验 Display 的独立装包边界：默认 alpha-check 的 Android APK 不含
可选 GameStream，因此另执行带 stage 参数的 verifyGameStreamDebugApk，通过
（42 tasks）。原生输入脚本核对 269 个源码文件、2 个集成文件及 APK 库字节，
生成 `target/release-evidence/native-inputs-20261002/`。详情见 RELEASE_EVIDENCE.md。
此次仅补充证据脚本和文档，没有重复运行此前已通过的完整 Rust 测试。

不能把当前结果称为“五阶段全部验收通过”或“所有功能已经稳定”。摄像头普通
权限缺口、实机可靠性、系统安装、OpenSSL/Windows 原生分发材料和生产签名仍未关闭。
这些限制按用户确认保留；未执行驱动部署、APK 安装、生产签名、提交或公开发布。

## 最新连续推进：交付材料与文件安装闭环

新增 `scripts/build_release_inventory.py`、`scripts/release_inventory.gradle`，只读
解析锁定的 Cargo/pnpm、Android debugRuntimeClasspath 及已有第三方集成记录，
生成 CycloneDX 1.5 pre-build 清单、依赖图、许可证文本/索引与证据缺口。没有新增
生产依赖；使用本机已有 PyYAML。组件引用唯一性和依赖闭合检查通过。

产物 `target/release-evidence/inventory-20261002-final/`：674 个组件/集成记录，
901 份许可证文本（含重复许可）；117 项证据不完整，具体列表在 evidence.json。
此清单包含开发/其他平台依赖和集成声明，未声称它是实际分发包的完整 SBOM，
也未把缺失项当成可忽略或许可证法律结论。完整范围说明见 RELEASE_EVIDENCE.md。

新增一个必要的安装边界检查 `scripts/test_local_debug_installer.ps1`，复用现有
假包和升级测试。真实 NSIS 安装器只向工作区隔离目录复制假文件，不创建快捷方式，
验证拒绝覆盖、可恢复移除与还原；没有运行任何假 EXE，没有驱动/服务/APK 操作。
结果通过，日志 `test-results/alpha-file-installer-check.log`；安装模板中的旧
“关窗后升级”提示已改为“退出节点后升级”。测试假文件留在 target 中供复核。

Rust/Android 产品代码本轮未改动，因此没有重复此前完整软件 Gate。阶段 3 的实机
以及阶段 4 的系统安装、生产签名、实际装包对应源码/许可证验收仍未完成。


## 最新连续推进：独立 Windows Host 与完整 Local API

此前同进程生命周期方案已继续迁为独立进程。`node_process.rs` 的 `--node-host`
入口只创建 Host，不创建 Tauri 应用/WebView；普通入口只创建 UI 代理。53 个既有
命令进入 `host_commands.rs` 的有限分派表，前端接口与参数形状不变。
`HostState` 仅在 Host 内使用，UI 不持有 Runtime。现有会话静态状态也只在 Host
执行，关闭 UI 不会复制或销毁这些状态。Core 对象模型和平台媒体实现未重做。

Local API 复用 `platform/windows/capyio-service/src/local_pipe.rs`，采用精确用户
SID ACL、登录会话命名、远程拒绝、首实例保留和服务端 EXE 路径校验。协议版本、
1 MiB 控制帧、四个固定 worker、非阻塞客户端读取与有界响应确认已经接入。
原系统服务的 4 KiB/10 秒契约保留，没有新增第三方依赖或放宽提权/驱动权限。

本机运行日志 `test-results/node-host-lifecycle.log` 已通过：

- UI PID 111988 与 Host PID 111344 分离；快照读取成功。
- 未知命令、错误版本及错误 Route 参数被拒绝。
- 正常关窗使 UI 退出，Host 111344 仍响应。
- 重开 UI PID 59448 接回同一 Host；结束本轮 UI 进程后 Host 快照仍可查询。
- 再次打开 UI，通过菜单退出节点，UI 返回 0，Host 也退出。

最终 `cargo xtask alpha-check` 已通过，完整日志
`test-results/node-host-final-gate.log`：Rust 汇总（含 adapter smoke）1116 passed、
0 failed、31 ignored；前端构建及 88 项测试通过；Android capyioCheck 92 tasks 成功。
既有 MSVC 链接提示和前端 HMR 端口占用日志仍记录在案，不声称日志零告警。
构建和严格 Clippy 也通过，见 `node-host-build.log`、`node-host-clippy.log`。
本轮扩展已有生命周期脚本，不引入测试框架或大批实现镜像测试。
升级/移除脚本原有运行进程检查可覆盖 Host（使用相同 EXE）；操作提示和交付文档
已改为必须“退出节点”，仅关窗不够。新目录 UI 不接受旧目录 Host。

这些本机检查未启动物理 Route。整个 Host 重启后的自动拨号、开机启动没有加入；
设备缺席导致的实机并发/长测/恢复、系统安装和生产签名验收仍按用户决定延期。
仍在使用的 compatibility 路径未在缺少物理等价证据时删除。

下方保留历史步骤记录；其中同进程 Host/跨进程 IPC 未完成等描述已由本节取代。

## 最新结果：无窗口宿主与媒体诊断

用户已选择“关窗继续运行，重开界面接回原节点”。现有 Windows 入口在创建 Runtime
前进行同用户/登录会话单实例判定；关窗销毁窗口与 WebView，节点进程继续运行。
重复启动只发送无载荷的唤回事件，原进程重建窗口；“节点 → 退出节点（停止共享）”
正常退出并执行已有宿主清理。没有新增第三方依赖、驱动操作或 APK 安装。

主要文件：`apps/desktop/src-tauri/src/background_host.rs`、`lib.rs`、
`desktop_host.rs`、`route_use_coordinator.rs`、`route_readiness.rs`、
`native_audio_route.rs`、`camera_route.rs`、`display_workflow.rs`、`diagnostics.rs`，
以及 `scripts/test_desktop_host_lifecycle.ps1`。

媒体诊断经 Host 和连接控制队列读取最近观测，连接表锁在等待前释放。
快照包含 Session/Route/epoch、时间戳、worker/媒体就绪、已观察帧数；没有把缺失
指标伪造成零值或实时吞吐。显示也已接入共享就绪证据。每连接最多 32 条观测、
最多 16 个活动连接；超出连接上限返回明确错误。

已执行：

- 最新全部改动后的 `cargo xtask alpha-check` 通过：Rust format/check/严格 Clippy/
  workspace tests、仓库/文档/manifest/Adapter、前端构建与 88 项测试、Android
  capyioCheck（92 tasks）。日志 `test-results/alpha-consolidation-final-gate.log`。
  MSVC 链接提示与前端 HMR 端口占用日志仍有记录；各检查退出成功。

- 严格 Desktop Clippy 通过；`test-results/background-host-clippy.log`。
- Desktop 单元测试 140 passed、10 ignored、0 failed；
  `test-results/background-host-desktop-tests.log`。新增单实例唤回/释放边界测试。
- `cargo build -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol`
  通过；`test-results/background-host-build.log`。保留已记录的 MSVC 链接消息。
- 本机生命周期检查通过；`test-results/background-host-lifecycle.log`：PID 84204
  在窗口销毁后存活，重复启动退出 0，原 PID 重建窗口，菜单退出 0。
  初次检查发现重建窗口没有退出菜单，改为重新绑定菜单后重跑通过。
  检查不启动物理 Route，因此没有验证媒体跨关窗连续性。
- 文件包完整性、升级/回滚和损坏拒绝检查已通过；
  `test-results/alpha-file-upgrade-check.log`。这不是驱动/服务安装验收。

当前仍为同一进程内的 Host 与窗口 Shell；窗口生命周期解耦已经落实，但跨进程
Local API、进程退出后自动拨号并未实现。普通权限摄像头仍是缺口。物理设备不在
用户身边，本轮不声称完成长期、并发、恢复、安装签名或发布验收；兼容路径仍需
物理等价证据才能删除。发布许可保持关闭。

下方是此前各步记录；其中“尚未实现关窗存活”等描述属于当时状态，以本节和
CURRENT_ARCHITECTURE.md 为准。

## 本轮连续推进：媒体协调与就绪证据

RouteUseCoordinator 已接入控制连接循环，拥有音频/摄像头/显示协调器并统一
启动、停止、授权分派、状态接收和连接关闭清理。保留显示失败局部清理、音频/
摄像头错误向控制连接传播的现有区别，以及原轮询和关闭顺序。
RouteReadiness 已用于音频/摄像头，明确 worker 就绪、对端媒体确认与本机计数
进展；分别保留扬声器、麦克风、摄像头原有激活条件。

累计宿主迁移的 `cargo xtask alpha-check` 通过，完整日志
`test-results/alpha-consolidation-final-gate.log`。该 Gate 开始于本节媒体协调修改
之前；后续改动另经严格 Clippy 和 Desktop 单元测试验证：139 passed、10 ignored、
0 failed，日志 `test-results/architecture-consolidation-desktop-tests.log`。
本节新增一个就绪证据边界测试，不增加测试框架。MSVC 链接消息仍为已记录例外；
前端测试出现 HMR 端口占用日志，但测试退出成功，不能宣称完整日志没有告警。

用户已确认暂不做实机验证，Windows 后续以本机为目标，不使用 VM。
五阶段的实机、安装、签名与发布验收继续保留未完成状态；独立宿主关窗/重接策略
正等待用户选择，现有默认退出行为未改变。

## Windows 同进程宿主访问边界收口

显示目录、显示启动和停止迁入 desktop_host/display.rs。全部 DesktopHost 资源
字段改为私有，删除 AppState 的 Deref 透传；Tauri 命令通过显式 Arc 句柄调用
宿主方法。编译器现可阻止 UI 命令绕过方法直接获取宿主的锁与 worker 字段。
远端设置已有独立会话状态存储，不存在 DesktopHost 字段访问，未增加空转包装。

严格 Clippy 通过，日志 `test-results/desktop-host-boundary-clippy.log`。
本次没有新增测试、权限、依赖或生命周期行为。此项完成同进程字段封装，不等于
独立 Headless 进程、跨进程 Local API 或关窗存活；进程级静态状态也未全部迁入宿主。

## Windows 无文件配对与资源授权宿主化

配对操作、状态查询和恢复流程迁入 desktop_host/enrollment.rs，原文件保留 Tauri
适配及既有存储 worker。Enrollment 字段私有，参数、PIN 处理、原证书恢复与锁
顺序保持不变。资源授权处理迁入宿主连接模块，不增加授权策略或状态缓存。
诊断函数不持有宿主资源，保留原只读实现，避免增加无意义包装。

严格 Clippy 通过，日志 `test-results/desktop-host-enrollment-clippy.log`。
本轮没有新增测试、依赖、功能或防御层，以编译及接线核对验证迁移。
显示/远端设置仍需访问边界整理；跨进程 API、统一就绪/指标和实机交付验收未完成。

## Windows 接收连接与配对目录宿主化

配对列表、凭据检查、发现候选、监听状态、监听启动和配对邀请六个入口迁入宿主
连接模块，接收后的 Session 接入辅助流程也移入该模块。手柄使用查询/启停三个
入口迁入宿主输入模块，paired_listener/gamepad_lab 字段改为私有。
原协议、凭据读取、握手超时、锁顺序与控制消息保持不变；Tauri 仅转发原 DTO。

严格 Clippy 通过：`test-results/desktop-host-listener-clippy.log`。首次私有化发现
两个剩余手柄直接访问，迁移对应职责组后复查通过。本轮没有新增测试或校验层，
采用编译、静态检查和接线核对验证。完整跨进程 API 与实机验收仍未完成。

## Windows 主动连接与预检边界

主动连接的两个入口、连接计划执行及失败清理移入 desktop_host/connections.rs。
Tauri 只调度阻塞任务；任务持有 DesktopHost Arc，不再借 AppHandle 查询 AppState。
原握手、目录注册/交换、锁顺序、单 Peer 限制和失败协调保持原样。
手柄预检的 single-flight 准入与结果写回迁入宿主方法，并发标志改为私有；准入
仍发生在异步调度前，原阻塞探测与 await 时序保留。

严格 Clippy 通过，日志 `test-results/desktop-host-connections-clippy.log`。
未新增测试或校验层；采用编译与调用点核对验证本次保持行为的迁移。
接收连接、配对及诊断仍存在直接宿主字段访问，跨进程 API 与实机验收尚未完成。

## Windows 输入职责组迁移

将手柄/IMU 的 12 个同步命令迁入 `desktop_host/input.rs`，并将原有宿主 IMU
停止方法归入同模块。Tauri 入口只调用对应宿主方法；沿用原参数、错误提示、锁
顺序与生命周期操作。IMU Route、快照和 worker 字段改为私有，未新增状态机、
依赖或测试。异步手柄预检与连接编排仍有 UI 侧访问，不宣称宿主 API 已全部完成。

严格 Clippy 通过，日志 `test-results/desktop-host-input-clippy.log`；现有两个
loopback worker 生命周期测试通过，日志 `test-results/desktop-host-input-tests.log`。
这些检查不替代实机输入和 UI 关闭后的存活验收。

## Windows 共享与音频操作宿主化

将 set_route、set_local_capability_shared、set_remote_capability_use、
get_quick_actions、invoke_quick_action、get_audio_endpoints、select_audio_endpoint
七个入口的业务编排迁入 DesktopHost。Tauri 保留 UUID 参数解析和原 DTO，宿主
负责物理 Route 入口限制、共享变更发布、连接选择和音频资源访问。
audio_share/microphone 字段改为私有，编译器约束后续命令通过宿主方法访问。
同时恢复前轮机械迁移误改的锁错误文案（self lock → state lock）。

严格 Clippy 已通过，日志 `test-results/desktop-host-actions-clippy.log`。
既有 `capability_catalog_separates_local_sharing_from_peer_inventory` 测试通过，
日志 `test-results/desktop-host-selection-check.log`；首次筛选未命中测试后已更正名称。
不新增测试、状态机或依赖；完整 Headless/Local API 与实机验收仍未完成。

## Windows 宿主方法接入

将快照读取、发现启停、Peer 断开、配对监听停止、IMU 停止和演示重置共 7 个入口
从 Tauri 命令实现迁入 DesktopHost。对应命令保留参数转换与原返回 DTO；worker
停止/join、锁顺序、错误处理均沿用原实现。发现、音频和手柄 worker 字段改为私有，
编译器约束 UI 不再直接访问这些资源。未增加接口权限、IPC、校验层或测试。

`cargo fmt -p capyio-desktop` 与
`cargo clippy -p capyio-desktop --all-targets -- -D warnings` 通过，日志
`test-results/desktop-host-api-clippy.log`。本次为同进程调用迁移，验证采用编译、
严格 Clippy 与接线核对，未重复执行前轮 worker 测试或实机回归。
其余命令的直接字段访问、跨进程 Local API 和 Headless 实机验收仍未完成。

## Windows 宿主所有权提取

新增 `apps/desktop/src-tauri/src/desktop_host.rs`，集中生产 Runtime、控制目录与
连接、配对存储 worker、音频/手柄轮询的初始化和销毁。`lib.rs` 的生产入口先
创建 DesktopHost 并保留进程引用，Tauri AppState 只持有 Arc 宿主句柄。
原启动参数、轮询周期、停止/join 顺序和命令 DTO 均保留；未新增后台驻留行为。

严格 Clippy 通过：`cargo clippy -p capyio-desktop --all-targets -- -D warnings`，
日志 `test-results/desktop-host-clippy.log`。现有 worker 生命周期与断线 epoch
测试通过：`cargo test -p capyio-desktop --lib tests::loopback_`，2 passed，日志
`test-results/desktop-host-worker-check.log`。未新增测试；这些测试验证原 worker
行为，不证明物理设备等价或关窗存活。

当前是宿主所有权边界提取，命令仍以同进程字段访问调用宿主。独立宿主进程、
Local API、关窗运行和重连尚未完成，五阶段计划不据此标记完成。

## Windows 原生媒体实验端点策略集中

音频和摄像头现在共用 `media_lab_endpoints.rs`，集中出口地址探测和已批准的
Windows/Y700 地址对限制。使用 IPv4 值比较替代字符串转换，保留原网络探测顺序、
拒绝范围及能力专属提示。未扩大网络支持范围，未改变媒体加密或授权契约。
音频原先位于地址白名单检查之后的 IPv6 错误分支不可达，已随重复代码一并移除。

`cargo clippy -p capyio-desktop --all-targets -- -D warnings` 通过，日志
`test-results/media-lab-endpoints-clippy.log`；未新增测试或执行实机回归。
该提取保持既有行为，以编译、严格 Clippy 和调用点核对验证。

## Windows 协调器公共实现：端口租约

新增 `apps/desktop/src-tauri/src/route_port_leases.rs`，已用于音频与摄像头的
远端租约申请、本机独占租约申请和双端续约。删除原重复调用，保留 Acquire 接受
Acquired/Renewed、Renew 只接受 Renewed 的语义，以及原 TTL、时间戳、执行顺序
和各能力错误提示。共享对象只携带 Route/Port 标识，不缓存授权或另建状态机。
媒体就绪判断、启动和调度周期继续由各能力负责；没有新功能、依赖或协议变更。

验证：`cargo clippy -p capyio-desktop --all-targets -- -D warnings` 通过，日志
`test-results/route-leases-clippy.log`；`cargo test -p capyio-desktop --lib control_exchange`
既有 6 tests passed，日志 `test-results/route-leases-tests.log`。未新增测试。
初次 Clippy 指出两个冗余字段名，修正后复跑通过。
这完成本次租约收敛切片；Headless Host、统一媒体就绪/指标、实机长测与交付验收
仍未完成，不能声明五阶段已全部落实。

## Windows 协调器公共实现：延迟状态交接

新增 `apps/desktop/src-tauri/src/route_status_inbox.rs`，音频与摄像头协调器均已使用。
移除两份重复的队列入队、Session 批量取出和清理代码，保留分别持有的 32 项队列、
同 Session/Route 最新状态覆盖及原有锁失败行为。能力筛选、epoch/授权检查、
媒体就绪、续约与启动/停止仍由原路径处理，不新建协调状态机或依赖。

必要验证已通过：

- `cargo test -p capyio-desktop --lib route_status_inbox`：1 passed，覆盖容量、
  满容量覆盖、Session 隔离及定向清理；`test-results/route-status-inbox-check.log`。
- `cargo test -p capyio-desktop --lib native_audio_route`：现有 3 tests passed；
  `test-results/route-inbox-audio-check.log`。
- `cargo clippy -p capyio-desktop --all-targets -- -D warnings`：通过；
  `test-results/route-inbox-clippy.log`。
- `cargo fmt -p capyio-desktop`、`git diff --check` 与仓库结构验证通过；
  `test-results/route-inbox-repository-check.log`。

本轮只补一个共享边界测试，没有重跑全仓或实机矩阵。完整统一协调器、
Headless Host 和五阶段实物验收仍未完成；本节不声明整个优化计划已完成。

## 累计模块拆分收口：控制与配对状态

AndroidControlModule 已接入 Service 启动、配对完成后的重载、主动连接、Binder
状态查询与销毁路径。集中管理配对目录、凭据加载和控制监听器，保留原凭据清零、
单有效 Peer 重载限制、UI 元数据白名单；PIN 首配和恢复仍使用 AndroidFilelessPairing。
Service 销毁仍先关闭控制监听器再关闭 Runtime，未引入新的同步层或状态机。

本次集中验证累计八个职责模块的接线与现有 Android 软件契约：

- `gradlew.bat --project-dir platform/android capyioCheck`：通过，92 tasks，
  16 executed / 76 up-to-date；日志 `test-results/consolidation-android-check.log`。
- `python scripts/validate_repository.py`：通过，88 unique Requirement IDs；
  日志 `test-results/consolidation-repository-check.log`。
- 控制模块单项 Java 编译、Lint 与既有 composition 333 assertions 通过；
  日志 `test-results/control-module-check.log`。

未新增重复测试、依赖或 manifest 声明，未安装 APK/驱动、未签名或发布。
Android 模块化当前切片已收口；阶段 3/4 实物验收和阶段 5 的 Headless Host、
统一 RouteUseCoordinator/readiness/metrics 仍未完成，旧路径也未据此删除。
以下全仓 Gate 结果属于此前基线，不替代本节的累计 Android 验证或实机验收。

## 后续小步重构：Android 通知呈现

AndroidNodeNotification 已接管通知渠道创建、状态文案和 PendingIntent 构建。
Service 仍负责是否进入前台、foreground service type 和停止流程；通知 ID 4101、
原渠道 ID、打开 Activity 与停止全部动作、immutable 标志均保留。未修改 manifest、
权限或后台运行策略。本次不引入额外状态机或通知配置层。

Java 编译、Lint、差异检查通过；日志 `test-results/notification-module-check.log`。
纯代码迁移未新增测试，未执行实机通知验收。

## 后续小步重构：Android 目录模块

AndroidCatalogModule 集中管理共享选择、启动恢复、speaker 首次迁移、偏好持久化
及发布 JSON 编码，已接入 Service 初始化、目录投影和 Binder 选择入口。
沿用 AndroidSharingSelection 先更新 Runtime 再提交选择的原有顺序；Service
保留 Runtime 生命周期、相同目录去重与前台服务管理。不新增目录模型或缓存层。

Java 编译、Lint、既有 capabilityCompositionTest（333 assertions）和差异检查
通过；日志 `test-results/catalog-module-check.log`。未新增测试或执行实机回归。

## 后续小步重构：Android 手柄模块

AndroidControllerUseModule 已接入 Service 轮询、Binder 启停/状态读取，以及旧接收
端点广告的 Route 所有权查询。模块集中管理 Route ID、启动时间、首帧/超时判定和
状态 JSON；Service 保留 gamepad/IMU 生命周期，原有 receiver 凭据过滤继续生效。
重复输入停止顺序集中到 stopInput，未增加状态机、校验层或依赖。

Java 编译、Lint、差异检查通过，日志 `test-results/controller-module-check.log`。
未新增测试或重跑全仓检查；本轮没有实机回归结果。

## 后续小步重构：Android 资源授权模块

AndroidResourceConsentModule 已接入 Service 的周期授权处理、Binder 记住授权
和清除入口。模块集中处理配对代次、资源键、旧触控板授权迁移及持久化；每次使用
Service 传入的当前目录，不另外缓存信任状态。原有 ResourceConsentMemory 规则、
Runtime 最终授权、存储名称及同步锁保持不变，没有增加授权校验层或通用框架。

Java 编译、Lint、既有 capabilityCompositionTest（333 assertions）及差异检查
通过；日志 `test-results/consent-module-check.log`。既有测试验证原规则，模块
接线由编译和代码核对验证；没有新增重复测试或宣称实机验收。

## 后续小步重构：Android 音频模块

AndroidAudioUseModule 已接入 Service 轮询、Binder 和销毁路径，集中管理双向音频
Route job、端点准备、guard/Socket 释放、实际帧就绪判断及 UI 状态投影。
硬件仍由 Service 原有 controller 与 Adapter 启停，模块通过 snapshot/start/stop
三个回调使用原有生命周期；不新增状态机，麦克风发布与权限要求保持原样。
资源释放集中到 job.close；Service 销毁仍先撤销 Route 资源，再停止硬件控制器。

仅执行 Java 编译、Lint 和差异检查，通过；日志 `test-results/audio-module-check.log`。
未新增重复测试、未重跑全仓 Gate、未做实机验收。

## 后续小步重构：Android 显示模块

将显示质量探测、启动记录交换、接收器启动/停止、帧呈现回报及状态投影移入
AndroidDisplayUseModule。已接入 Service 的轮询、Binder 和销毁路径；异步回调
继续使用同一个 Service monitor，保留现有 operation 匹配及销毁检查。
模块集中拥有 operation ID、receiver-opened 与状态 JSON，不新增通用状态机。
启动记录与 ACK 的使用后清零继续保留，权限、协议与系统 Service 声明未改变。

针对性 Java 编译与 Lint 通过，见 `test-results/display-module-check.log`；
未添加重复测试、未重跑全仓 Gate、未做实机验收。

## 后续小步重构：Android 摄像头模块

按用户“快速推进、避免过多防御性编程和测试”的要求，将摄像头 Route 编排从
AudioNodeService 移入 AndroidCameraUseModule，已接入 Service 轮询、Binder
启动/停止、UI 状态投影和 onDestroy 清理。模块拥有 Route ID、启动时间与状态
JSON；Service 保留同步锁、系统生命周期和资源发布。保留原权限检查、首帧判断、
超时与故障处理，不引入第二套状态机、新依赖或 Service 声明。

本次仅运行 `:app:compileDebugJavaWithJavac :app:lintDebug`，通过；日志为
`test-results/camera-module-check.log`。行为等价迁移未新增重复测试；未执行实机回归。
下文完整 Alpha Gate 结果属于此次模块拆分之前的基线，不冒充本次重跑结果。

## 范围与结论

本轮不新增硬件功能，不改变 Core/网络协议、权限或系统 Service 声明。
保留开始时工作区已有的配对、UI、依赖和文档改动；没有提交、推送或发布。
五阶段尚未全部完成：工程基线与功能冻结先落地，完整实机可靠性、系统安装发布
和大型架构迁移仍是未关闭 Gate，详见 ALPHA_CONSOLIDATION.md。

## 修改

- `platform/android/capyio-jni/tests/catalog_runtime.rs`：用类型、方向、Adapter
  所有权及完整快照不变性替代数量断言，覆盖成功替换及拒绝后的事务性。
- `platform/android/capyio-jni/src/lib.rs`、`display_catalog.rs`：Android 专用字段
  按目标平台编译，保留可移植目录/选择策略测试，未使用全局 dead_code 抑制。
- `apps/desktop/src-tauri/src/android_receivers.rs`：按用户确认的暂定直连契约，
  删除无人调用的自动提升 Broker 辅助代码；保留 Broker IPC 和状态路径。
- `microphone_quick_action.rs`：将兼容 DTO 投影从已安装原生服务探测中分离；
  三个测试不再因开发机上存在真实服务而覆盖自己的夹具。
- `capyio-control-link`、`display_workflow.rs`：大型 RouteStatus 分支间接存储；
  `capyio-enrollment`：用 EnrollmentAttempt 汇集单次尝试上下文；显示服务命名
  内部 Commit 消息类型。保持现有授权、传输和生命周期语义。
- Clippy 的行为等价建议及 `cargo fmt --all`：清理嵌套条件、Copy 类型 clone、
  数值检查和格式漂移。格式整理范围比功能改动范围大，未丢弃既有未提交内容。
- Android 目录投影移除高于 minSdk 的 Stream.toList；启动发布偏好使用 apply。
  控制设置事务因需要同步持久化确认而保留 commit，仅对该方法定点说明 Lint 例外。
- `xtask/src/main.rs`：增加不跳过 Desktop/frontend/Android 的 alpha-check。
- `scripts/validate_repository.py`：跟进摄像头计数迁移；区分 Service 控制编排与
  音频 Adapter，继续拒绝 Adapter 内普通日志和直接 send/receive。此静态规则
  不证明所有传递调用都满足实时约束。
- `release/alpha-scope.json` 与共享 `CapabilityCatalog.vue`：冻结范围并显示
  Experimental。`publicationApproved` 仍为 false。
- CURRENT_STATUS、CURRENT_ARCHITECTURE、ADR 1509 与五阶段计划建立当前导航；
  旧构建记录保留为历史证据，不整体搬移或改写。

## 已执行验证

- `cargo check --workspace --all-targets`：通过，无源码告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace`：1112 passed、0 failed、31 ignored；ignored 实机用例没有执行。
- `cargo test -p capyio-desktop --lib`：137 passed、0 failed、10 ignored。
- `pnpm --filter @capyio/desktop build`：通过。
- `node --test scripts/*.test.mjs`：88 passed、0 failed。
- `cargo xtask android-check`：通过；92 个 Gradle task，包括 ARM64 JNI、APK、Lint
  和 Java 契约测试；没有安装 APK。
- 文档、manifest 和完整 repository 静态验证：通过。
- 最终组合 `cargo xtask alpha-check`：通过，包含 Adapter smoke 的 Source/Sink
  生命周期与 crash isolation；最终 Android 构建 92 task 成功。
- `cargo xtask doctor` 与 `cargo xtask demo`：通过。doctor 的可选工具 adb、
  msbuild、windbg 在当前 PATH 未发现；ADB 可通过已有 SDK 绝对路径运行。
- `git diff --check`：通过。未运行托管 CI，未提交或发布。

原始日志保存在忽略目录 `test-results/alpha-*.log`。首次沙箱测试因回环 TCP
WinSock 10013 失败，首次 Android 构建因已安装 NDK clang.exe 执行权限失败；
获准在沙箱外执行相同测试/构建后完成。不是缺少 Rust Android target。
`adb` 不在 PATH；使用已有 SDK 的绝对路径执行 `adb devices` 后设备列表为空，
因此本轮无连接设备的物理回归证据。

完整 Rust 测试链接 cdylib 时，工具链把 MSVC 正常“创建库”输出报告为
`linker_messages` warning（JNI/Desktop 两处）。严格 check/Clippy 无源码告警，
但不能将此记录称为所有编译日志绝对零 warning；没有通过全局禁用 warning 隐藏。

## 未关闭风险

没有证明更新后的包在 Y700 上的物理等价性、长测、并发、睡眠或断网恢复。
没有执行驱动/服务部署、生产签名、新机器完整安装或卸载，不具备公开发布资格。
完整 Headless Host、Android Service 拆分、统一 RouteUseCoordinator/readiness/metrics
仍待后续迁移，不能把本轮局部边界清理解释成上述架构工作全部完成。
