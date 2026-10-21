# CapyIO 当前状态

2026-10-19：当前源码已整理为独立的隐私清理同步快照，云端入口、清理范围及
本次验证见 [源码交接](CLOUD_SYNC_HANDOFF_20261019.md)。以下保留既有功能与实机
验收记录；本次同步不将任何待验项目改计为通过。

更新：2026-10-03。主要能力已实现，当前交付范围为可安装的本地 Alpha 候选，
尚非公开稳定版。本文汇总当前结论；历史报告保留各自的版本和验收范围。

## 使用与交付

- [本地候选交付](LOCAL_ALPHA_CURRENT.md)：当前安装器、APK、哈希和验证边界。
- [Windows 入门](FIRST_RUN_WINDOWS.md)：安装文件、检查已有组件、配对及使用顺序。
- [当前架构](CURRENT_ARCHITECTURE.md)：模块职责与边界。
- [发布准备](ALPHA_RELEASE_PREPARATION.md)：后续交付门槛及历史批次证据。

最新候选目录为 `target/alpha-profiles/20261003-141621-815757`。Core 的八个 Windows
程序和 Android APK 来自同一固定源码候选的独立构建，Extras 明确复用旧实验产物。
安装器只管理应用文件，不部署驱动、服务或相机注册。升级前应显式退出旧节点；
关窗后 Host 继续运行，重新打开界面会接回原 Host。

## 已实现与已验证

主流程为 Pairing/Connection/Catalog，主要能力为 Speaker/Microphone/Camera/Touchpad；
Gamepad/Haptics/Display/IMU 仍为 Experimental。PIN 配对及恢复进展见
[配对记录](PAIRING_UX_PROGRESS_20260930.md)。摄像头采用已批准的管理员 Host
直接投影路径，普通权限支持尚未完成。网络范围为可信 LAN/Tailscale；控制 TLS
不能证明所有媒体具有统一应用层加密。

Windows UI 是独立 Host 的客户端，53 个现有命令经当前用户/会话的有界 Local API
调用。音频、摄像头、显示及输入 Route 由协调模块管理，控制连接循环已独立。
Android 保留一个系统 Service，内部划分控制、目录、授权、媒体和通知职责。
未增加硬件能力或变更内核驱动边界。

界面已统一导航、资源卡片、PIN 配对及媒体操作页；诊断默认折叠，活动资源保留
停止入口。完成桌面/窄屏演示预览和相关无障碍检查。最新候选包含这些界面改动。

此前 10:42 候选的软件 Gate：Rust 1116 passed、0 failed、31 ignored；前端 89 项、Python
交付工具 10 项检查通过；Android 92 项任务完成。固定的 1724 个源码输入在独立
构建后哈希未变。真实安装器已完成隔离目录中的文件安装、升级、精确清单回滚、
可恢复移除与恢复；未将本批 APK 安装到设备，也未验证本批实机媒体行为。

此前 Windows + Y700 实机证据包括四项主能力两小时并发、独立停止，以及摄像头
worker、控制连接、Android 进程各 10 次异常恢复。范围见
[实机记录](ALPHA_Y700_REGRESSION_20261002.md) 与
[补验记录](ALPHA_FOLLOWUP_20261003.md)。这些旧批次结果不能自动覆盖新产物哈希。

## 尚未验收

| 项目 | 当前边界 |
| --- | --- |
| 最新候选实机回归 | 构建、文件完整性与安装生命周期通过；本批媒体回归未执行 |
| 锁屏 / 睡眠 | 按用户要求暂缓，完整恢复矩阵未完成 |
| 新机器部署 | 仍依赖已部署的实验主机组件；未完成新机器安装验收 |
| 普通权限摄像头 | 管理员 Host 路径可用；普通权限缺口保留 |
| 公开源码审查 | 最新导出扫描 356 项，10 项说明仍匹配哈希，346 项待审 |
| 许可材料 | 清单 674 个组件、901 份文本、117 项证据不完整；包含非分发依赖，不能直接视作违规数量 |
| 公开发布 | 生产签名、实际分发依赖审查及发布审批未完成；publicationApproved=false |

后续优先验证最新候选和收敛交付材料，避免反复改变已冻结的 UI 或扩展能力。
[五阶段计划](ALPHA_CONSOLIDATION.md) 的软件部分与本地候选可以收口，物理验收及
公开发布门槛继续独立记录，不将待验项计作通过。

## 本轮维护收敛

修正 Windows 入门文档沿用 MicYou 主流程的问题，统一 README、当前状态与发布表
到最新候选入口。只读预检分别检查扬声器和麦克风，并区分设备错误、健康状态未知
与存在；配对标为未检查。该脚本的更新位于工作区，已生成候选保持原哈希；下一次
打包才会纳入，未原地改写已有交付物。

验证：只读预检的五项合成状态断言通过；仓库结构校验通过（88 个 Requirement ID），
`git diff --check` 通过。本机只读执行确认 Broker Running，但 CIM 设备清单查询返回
“拒绝访问”，设备存在和健康状态未确认。日志为
`test-results/prerequisite-state-regression.log` 与
`test-results/prerequisite-readonly-inventory.log`。本轮未改变 Runtime/UI 行为，未重跑
原生全量 Gate、重新构建安装包或执行设备安装。

配对界面异步顺序修复：用户命令使此前尚未返回的状态查询失效，防止取消后旧查询
重新显示等待状态/PIN 或过期错误；组件卸载后不再处理命令结果或发出配对完成事件。
仅调整 UI 响应顺序，配对协议、信任保存及后台生命周期不变。针对真实组件的合成
慢响应回归、现有连接/地址检查共 10 项通过；`corepack pnpm build`（含类型检查）
通过。新增回归由现有 `scripts/*.test.mjs` 自动纳入软件 Gate。本轮没有实机配对、
APK 安装或重新打包；当前交付包尚不包含该修复。

音频/摄像头面板沿用同一异步顺序原则：开始/停止期间暂停普通轮询，并使此前查询
失效；操作完成后刷新最新状态。离开页面使未完成查询失效，迟到的命令不再触发
额外查询或写入错误。卸载组件不调用资源 Stop，保留后台节点生命周期。真实组件
合成慢响应检查和现有资源卡片渲染共 3 项通过，前端类型检查/构建通过；并行 Vite
测试出现 HMR 端口 24678 占用提示，测试断言仍全部通过。记录位于
`test-results/media-poll-order-summary.txt`。尚未重新打包或实机验证。

扩展屏面板完成相同的命令/轮询顺序修复，避免开始/停止期间旧查询恢复旧会话和
画质参数；离开页面后的失败也不再写入界面。现有真实组件慢响应回归扩展到显示
面板，后台显示生命周期不变。全量前端检查首次发现两个生命周期截取测试缺少
新增 readGeneration 局部变量，补齐测试环境后 91 项全部通过（0 failed）；前端
类型检查/构建通过。日志：`test-results/ui-consolidation-regression.log`。
这是当前工作区的软件回归，不能替代最新安装包实机验收；本轮未重打包或部署。

最新更新候选已纳入上述 UI 响应顺序和只读预检修复，独立源码构建、APK 签名、包完整性与隔离目录真实安装通过。前文“尚未打包”是各切片完成时的历史状态，本批已完成打包；实机媒体验收仍未完成。当前证据见 [更新候选](LOCAL_ALPHA_CURRENT.md)。

界面产品化整理：Windows/Android 手动连接与 Windows 入站监听默认折叠，相关操作
反馈、需要处理的连接或活动监听会展开；PIN 保留为首配主入口，移除“首次必须导入
邀请”的过时说明。手柄与触控板诊断折叠，常用状态标题及体感/按键说明中文化；
设置页集中说明各项前置条件，手柄/显示入口提前标记实验范围及组件依赖。保留原始
技术标识供诊断，不改变功能成熟度、授权或系统部署行为。前端 91 项检查、类型检查、
构建通过；浏览器演示设置页的键盘展开/收起及 390px 无横向溢出检查通过。原生专属
表单仅有构建验证，尚无本轮设备交互验收。截图 `test-results/ui-settings-polished.png`，
日志 `test-results/ui-product-polish.log`。这批界面改动尚未纳入 13:13 候选包。

媒体状态反馈整理：音频、相机、扩展屏区分初次加载、查询失败与成功读取后的空目录。
查询失败提供“重新读取状态”，技术错误折叠展示，已有使用状态明确为上次读取结果；
首次读取前显示“尚未读取”，不声称未使用。显示查询失败时禁止依据旧目录启动，停止
入口不受该限制。真实组件合成失败/恢复回归并入既有检查，前端 92 项通过；类型检查、
构建及仓库校验通过。日志 `test-results/media-state-ux-regression.log`。本轮未重新
打包、部署或实机验收；13:13 安装包不包含本轮及上一轮产品化界面修改。

体验收口切片：手柄/显示详情以目标设备（手柄同时按目标模式）重建表单，避免沿用
上一目标的选择；活动会话仍以实际后台状态为准。手柄根据模式显示用途和前提，
显示页根据已有后端状态将接收组件未加载/服务不可用提示前置，旧版显示会话运行
或报错时展开诊断与停止入口。补齐常见状态中文标签。92 项前端检查、类型检查与
构建通过，日志 `test-results/ux-closeout-regression.log`。未新增依赖、权限或驱动操作。

上述产品化界面和状态修复现已纳入 13:46 更新候选，前文“未打包”为各切片当时状态。六步骤独立构建、APK 签名、完整性与隔离文件安装通过；当前前端 92 项通过。完整交付和仍未验收项见 [体验收口候选](LOCAL_ALPHA_CURRENT.md)。

Y700 当前已覆盖安装包含最新界面的完整实验 APK（保留显示接收组件），配对文件
哈希未变；用户确认首页/设置/手柄页正常进入。新增主页可选查询失败隔离及重试，
前端 94 项检查通过。Windows 13:46 候选的无窗口 Host 生命周期/API 只读检查通过。
本轮初始化修复仅已打入 Android，Windows 安装包尚未更新；媒体回归未计通过。
当前 Android 交付、安装证据及边界见 [Y700 体验验证](ALPHA_UX_DEVICE_CHECK_20261003.md)。

当前 Windows/Core 候选已包含主页初始化修复，网页文件与 Y700 已安装完整 APK 一致。独立构建改为最终产物/缓存分离，成功后清理自身编译缓存；完整打包、隔离文件安装及 Host 检查通过。本批以 [当前本地交付](LOCAL_ALPHA_CURRENT.md) 为准，前述“Windows 尚未更新”为历史切片状态。

显示兼容面板故障恢复修复：发送/接收状态独立结算，单端读取失败不再清空另一端
或上次已知的活动状态，保留停止入口；读取失败后禁止依旧状态启动。用户尚未提交
的观看选项不被轮询覆盖，普通刷新不抹掉操作错误；离开页面后不再写入异步结果。
真实组件合成回归验证独立失败、设置保留和失败后仍可停止，前端全量 95 项通过，
类型检查/构建、仓库及 diff 校验通过。日志 `test-results/display-recovery-regression.log`。
该切片仅在源码中，未重新打包或设备部署，不改变当前安装包哈希；本轮没有据此声称
实际显示故障恢复或媒体实机回归通过。


Windows 外壳错误反馈整理：单实例初始化、后台节点连接、WebView 创建与窗口唤回
失败显示原生中文提示和下一步操作，退出节点请求失败保留窗口并明确“未能确认
节点已退出”。复用已有 Windows API 依赖，不增加 WebView 权限，不改变 Host 的
独立生命周期。`cargo check --offline --locked -p capyio-desktop` 通过，日志
`test-results/windows-shell-feedback-check.log`。本次不添加只验证文案的测试；原生
对话框展示及故障路径尚未交互验收，未人为中断用户节点来制造故障。变更仅在源码中，
没有更新现有安装器或 APK。


2026-10-04 专项复验：普通权限摄像头接收、系统消费非占位画面、重开及退出通过；
音频在控制台会话恢复，Windows 采样与平板实际出声已确认。旧 MF 首次失败根因尚未
证明，保留历史记录。服务和 Android 修复包已部署，Windows 安装包尚未更新。
完整事实、日志和未完成项见 [专项记录](ALPHA_GAP_CLOSEOUT_20261003.md)。

2026-10-04：统一 Windows 安装新增可选签名驱动包接入（ADR 1513）。新机自动创建音频/VHF
Root 设备、安装编排和重启返回代码已实现；用户确认当前没有微软签名包，要求先完成代码
及材料清单。当前产物不含驱动，未做真实装卸，不能记为新机安装验收通过。
详见 [统一交付记录](UNIFIED_WINDOWS_DELIVERY_20261004.md) 与
[签名材料清单](WINDOWS_DRIVER_SIGNING_HANDOFF.md)。

### Linux touchpad source increment (2026-10-21)

ADR 1535 adds a bounded Type-B projector, exact Runtime/mTLS owner and optional
libevdev helper. Tests cover actual Android touchpad policy through authenticated
transport into fake Linux event recording, plus native-helper fake syscall
lifecycle checks. Hosted native compilation is a distinct gate. No real input
device, cursor or gesture was exercised; physical acceptance remains deferred.
This is a library-owned input path, not a default launcher, enrollment flow or
Linux desktop permission provisioning feature.
