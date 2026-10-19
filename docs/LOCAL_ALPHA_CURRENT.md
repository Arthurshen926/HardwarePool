# 当前本地候选与构建空间收敛（2026-10-03）

当前 Windows/Core 候选：`target/alpha-profiles/20261003-141621-815757`。
Windows 安装器：`CapyIO-local-20261003-141704-7272-setup.exe`。
SHA-256：`0d1b1624be5d707b49be86f971f58b8f035d0415914886ee993cb4d5a492653f`。
Android Core APK：`Core/android/CapyIO-debug.apk`，SHA-256：
`cc7cf46f631f36c555120ed938714e4ef169e0cd55ce574b2fab4f814b79845e`。

本批包含截至该次构建的界面整理及主页初始化故障隔离修复。构建工具变更不改变运行时
权限、硬件功能或配对规则。升级前应通过“节点 → 退出节点”结束旧 Host。

## Android 当前使用版本

Y700 已安装 `target/android-ux/20261003-recovery/CapyIO-full-debug.apk`，保留实验显示
接收组件。其网页 HTML/JS/CSS 共三个文件与本批 Core APK 逐字节一致，故没有再次
覆盖安装。完整 APK 的设备安装与用户确认记录见
[实机页面检查](ALPHA_UX_DEVICE_CHECK_20261003.md)。两种 APK 的原生组件配置不同，
不能把网页一致表述成 APK 完全相同，也不推荐用 Core 替换需要显示功能的完整 APK。

## 构建与验证

固定源码候选：`target/source-export/20261003-141235-795885`（1730 文件）。
独立构建报告：`target/source-build/20261003-141304-2586/result.json`。
六步骤通过，源码输入未变，九项最终产物保留在 `artifacts/` 并绑定哈希。
成功后删除本次 `cargo-target`，整次构建目录保留约 449 MiB，包含源码副本、最终
产物和日志。打包从保留产物完成，证明无需继续保存编译缓存。

新增 10 GiB 开工空间检查；关闭一次性构建的 Cargo 增量缓存；成功默认清理本次
编译缓存，可用 `-KeepBuildCache` 保留。失败现场和历史目录不自动清理。打包兼容
新旧报告布局，11 项交付工具检查通过，PowerShell 语法、仓库及 diff 检查通过。
前端沿用同一界面源码的 94 项通过记录，本轮未再跑原生全量测试/Android lint Gate。

Core 16 文件、Extras 60 文件完整性通过；真实安装器在本批
`installation-check/application` 无快捷方式安装并校验通过。新版 Windows Host
启动、状态/诊断读取、错误 API 版本拒绝及退出检查通过，未创建物理 Route。

日志：`test-results/retained-candidate-build.log`、`retained-candidate-package.log`、
`retained-candidate-installer.log`、`retained-candidate-install.log`、`retained-candidate-host.log`。

## 未完成项

媒体实机回归、普通权限摄像头、新机器系统组件部署及实验能力完整兼容/恢复仍未
完成；锁屏/睡眠按用户要求暂缓。安装器仍只管理应用文件，不部署系统组件。
公开许可与源码审查、生产签名和发布审批未完成，publicationApproved=false。
本批是本地 Alpha，不能视为功能与体验全面成熟的正式发布。


## 构建后的源码改动

当前安装包尚不包含后续两项修复：显示兼容面板的独立查询恢复与未提交选项保留，
以及 Windows 启动/唤回/退出失败的原生提示。前者前端 95 项检查和构建通过，后者
原生编译检查通过；原生错误对话框仍需交互验收。详情见 CURRENT_STATUS.md。
为控制磁盘占用，本轮没有为这些小切片再次生成整套候选。


## 系统服务专项验证版本

本机已单独获批部署普通权限摄像头共享内存分配服务 r2。2026-10-04 接收实机帧、
系统消费非占位画面及重开退出通过，音频控制台会话的采样与平板实际出声已确认。
Android 修复包位于 `target/android-ux/20261004-camera-fgs/`，已覆盖安装；
上述服务改动不在上方旧 Windows 安装器中，旧 MF 首次失败仍保留为未定因记录。
详见 [缺口专项收口](ALPHA_GAP_CLOSEOUT_20261003.md)。
