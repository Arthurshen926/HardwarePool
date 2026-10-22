# CapyIO 当前架构导航

更新：2026-10-03。本文描述当前实现边界；规范仍由 ARCHITECTURE、DOMAIN_MODEL、
PROTOCOL、SECURITY_MODEL 和已接受 ADR 定义。

| 边界 | 当前职责 | 尚未完成的收口 |
| --- | --- | --- |
| capyio-core / capyio-runtime | 确定性的 Node/Port/Route/Session、授权、lease、epoch、事件 | 保持现有模型，不重做 |
| control-host / control-link / control-tls | 已绑定 Session 的消息处理、配对 mTLS、目录和 Route 控制 | 全面自动重连与多 Peer 产品验收 |
| Windows Desktop | 独立 `--node-host` 持有 Runtime/控制/平台 worker；UI 仅代理 53 个既有命令到 Local API | 空闲进程生命周期及三媒体活动时正常关窗已验证；新构建包和更广泛故障场景仍需对应验收 |
| Windows Service / Broker / worker | 既有音频、显示等进程与投影边界 | 统一安装管理和系统组件验收 |
| Android AudioNodeService | 一个系统 Service 组合 Control、Catalog、Consent、Audio、Camera、Display、Controller 与通知模块；JNI 连接 Rust Runtime | Service 保留硬件与前台生命周期；跨能力协调尚未统一，权限/声明不变 |
| 各 Adapter | 专用音频、CAVC、触控板/振动 TLS、GameStream 等数据面 | 保留各自契约，不宣称统一媒体安全/互操作 |
| 交付工具 | Core/Extras 文件清单、源码候选与构建证据、组件操作计划；现有轻量测试接入 CI/Alpha Gate | 服务/驱动统一部署、公开源码与许可审查仍待完成 |
| shared Vue | 两端资源目录、授权、功能面板；Experimental 来自冻结清单 | 只呈现真实状态，不将 Starting 当作媒体已到达 |

AndroidResourceConsentModule 统一拥有记住授权的读写、应用和清除，以及配对代次
匹配；Service 传入当前目录和 Runtime 并保留同步锁。沿用原有存储键与授权规则。

AndroidCatalogModule 管理共享选择、启动恢复、首次 speaker 选择迁移、持久化与
发布目录编码。继续复用 AndroidSharingSelection 的 beforeCommit 事务顺序；
Service 保留 Runtime 替换、目录去重及前台生命周期。

AndroidNodeNotification 负责通知渠道、文案和按钮构建；Service 仍独占前台服务
类型计算及 startForeground/stopForeground。通知 ID、渠道 ID 和停止动作保持不变。

AndroidControlModule 拥有配对目录状态、凭据加载、控制监听器启动/停止、重载与
主动连接。Service 仍负责调用顺序和同步；销毁时先关闭监听器，再关闭 Runtime。
AndroidFilelessPairing 保留现有 PIN 首配与恢复流程，成功后调用控制模块重载。
凭据使用后清零，面向 UI 的 Peer 元数据仍采用原有字段白名单。

AndroidControllerUseModule 已接管手柄 Route 编排、首帧就绪和超时判断、状态投影。
Service 保留 gamepad/IMU 的硬件生命周期，通过启动模式、停止输入、状态通知三个
回调连接模块。旧的 receiver 广告兼容入口查询模块的 Route 所有权，避免重启活动发送器。

本轮真实边界变更：Android 专用状态按 target 编译；目录用语义/事务测试覆盖；
麦克风兼容 DTO 与本机服务探测分离；用户态控制队列的大 RouteStatus 采用 Box；
删除无调用者的摄像头自动提权启动辅助函数。保留已有 Broker IPC 和状态接管。

Windows 音频与摄像头协调器共用 RouteStatusInbox，实现按 Session/Route 合并状态、
按 Session 取出以及停止清理。两种能力分别持有队列，维持各自 32 项上限；
消息取出后仍走原协调器和 Runtime 的绑定/epoch/授权验证。

RoutePortLeases 集中音频与摄像头的远端租约申请、本机独占租约申请和双端续约。
它只保存 Route 与两端 Port 标识，不拥有授权状态；远端响应仍通过 control_exchange
关联，实际租约仍由 Runtime 管理。各能力保留自己的调度周期与媒体就绪条件。

音频与摄像头已共用 media_lab_endpoints，集中出口地址探测及批准的
Windows/Y700 IPv4 地址对；网络支持范围和媒体安全限制保持不变。

Windows 普通启动只创建 Tauri UI；`--node-host` 才构造 DesktopHost，完全不创建
Tauri 应用或 WebView。HostState 只在宿主内使用。全部资源字段私有，业务编排位于
`desktop_host/{connections,input,enrollment,display}.rs`，销毁顺序集中于 Host Drop。

`host_commands.rs` 是 53 个现有命令的唯一分派表，复用原有 DTO/参数转换和业务
方法；`node_process.rs` 管理宿主启动、命名管道服务、UI 转发与显式退出。
前端 CapyIOApi 契约不变。已有会话静态存储仅在 Host 执行，不随 UI 重开而复制。

Local API 使用当前用户 SID 和登录 Session 命名管道、精确 SID DACL、拒绝远程
客户端、首实例保留及服务端 EXE 路径校验。协议版本为 1，每帧最多 1 MiB，最多
四个服务 worker。客户端非阻塞读取最长 60 秒，服务端收发最长 2 秒；响应确认
代替无界 FlushFileBuffers 等待。平台原有服务管道保留 4 KiB/10 秒契约。
没有新增依赖、自动提权或数据面转发。不同安装目录不可共享旧 Host。

诊断入口经宿主请求连接 worker 的 RouteMediaSnapshot，等待前释放连接表锁。
快照标明采样时间及 Session/Route/epoch；音频是最近就绪观测，摄像头是最近轮询，
显示是最近 Presented 消息。它们不是统一实时速率，不推导丢帧或延迟。

RouteUseCoordinator 已接入控制连接循环，拥有音频、摄像头、显示和手柄的 Adapter
协调器，统一生命周期分派和关闭清理。授权、lease 与 epoch 仍由 Runtime 校验。
RouteReadiness 用于这三种媒体，按各 Adapter 要求区分 worker、对端媒体和本机帧
进展；它是固定大小的证据值，不是第二套 Route 状态机。手柄启停、授权、状态分派、反馈请求标志及断连清理也由同一入口持有；
触控板/振动由其持有的 InputRoutes 管理待授权/已启动 Route、续约时钟和数据 worker；
控制循环不再自行维护这些表。沿用原数据面、60 秒授权等待和既有续约周期。

Windows background_host 只管理 UI 单实例与窗口聚焦，不拥有 Runtime。关窗使 UI
进程退出；重开 UI 先连接原 Host。显式退出菜单发送 host_shutdown，Host 停止接收
新业务调用、等待 API worker 结束，再释放 Runtime 和平台资源。已经验证 UI 正常
关闭及进程中断后 Host 仍响应、重开连接相同 Host PID、菜单退出两进程。
本机检查没有启动物理 Route，不替代跨设备媒体连续性或长期稳定性验收。

系统自动启动和整个 Host 进程重启后的自动拨号未加入；普通权限摄像头、物理等价
与安装签名验收仍保留缺口，见 ADR 1509 和 ALPHA_CONSOLIDATION.md。

控制连接的命令调度、心跳/目录接收与 Route 清理集中于 `control_worker.rs`，
入口文件不再内嵌该 worker 循环；通过 RouteUseCoordinator 调用各能力协调器。


交付边界的当前验证入口见 [TESTING.md](TESTING.md)；旧包与当前源码构建证据见
[SOURCE_EXPORT.md](SOURCE_EXPORT.md)。Node Host 的有限实机结果见
[实机记录](ALPHA_Y700_REGRESSION_20261002.md)，不能外推为所有异常恢复已通过。


前端基础/既有面板样式位于 `apps/desktop/src/styles.css`，新版共享视觉层集中在
`apps/desktop/src/styles/interface.css`，由 main.ts 按基础在前、界面层在后的顺序
加载。停止按钮使用同一 danger-button 样式；资源按钮可见文字与无障碍名称由
同一展示函数生成，共享 checkbox 名称包含资源名。这里不持有设备生命周期。

Android secure-stereo foundation (ADR 1566) now separates shared Runtime lease
preparation from consume-once control sends and runs a bounded native receive
job with one exact write-ACK permit. Runtime/control generation and both original
claims remain authoritative; copying PCM is not playback progress. The Java
facade exposes native foundation methods only. Secure speaker selection,
AudioTrack ownership and a reverse-audio Node/console flow are still deferred;
existing speaker publication and legacy defaults are unchanged.
