# 体验优化与 Y700 安装验证（2026-10-03）

## 软件修复

主页首先读取必要的节点目录，再独立读取七项功能状态。单个体感、手柄或其他可选
查询失败不再丢弃成功的设备目录和其他功能状态；界面列出受影响功能并提供重试。
节点目录读取失败仍明确报错，不假装就绪。首次加载提示改为中文。

前端 94 项检查通过（含两项启动失败/恢复回归），类型检查与生产构建通过。
日志：`test-results/ux-recovery-regression.log`。

## Android 实机安装

用户提供更新后的无线调试端口，ADB 确认目标为原 Y700 TB323FU。原已安装 APK
哈希为 AF7EB79F0697E93A59F46DA1AD38BE8C825C9C44F6E1CCF200849A720E4E9D00，包含
实验显示接收组件，因此未用缺少该组件的 Core APK 替换。

从当前工作区通过 `build_integrated_android_debug.ps1` 构建完整 debug APK，复用
已有已验证 GameStream stage，不声称第三方源码全量重建。43 项 Gradle 任务完成，
完整 APK/JNI/显示 native 校验通过。新旧签名相同：
29307555e525e46a29150bb85d0b73e3fde7270865b215d07b5a95805cc41007。

交付文件：`target/android-ux/20261003-recovery/CapyIO-full-debug.apk`。
SHA-256：`9341e0bd4293b4c034fff9bf6de09d59afd00cfa28971a022d69bf6eeb0be622`。

沿用用户此前覆盖安装授权，`adb install -r` 成功，版本仍为 36 / 0.5.24-alpha.1。
安装前后 `no_backup/pairing/peers-v1.json` 哈希相同，未输出配对内容；未修改权限、
前台服务声明或独立 pairinglab 包。用户随后确认首页、设置和手柄页“正常进入”。
这属于用户确认的页面可进入证据，不是自动化逐控件或媒体回归。

日志：`test-results/ux-recovery-full-apk-build.log`、`y700-ux-install.log`。
配对摘要仅存忽略目录的 `y700-pairing-before-update.sha256` / `after-update.sha256`。

## Windows 软件检查

对 13:46 Core 候选的无窗口 Host 启动，验证状态与诊断读取、错误 Local API 版本
拒绝、显式 Host 退出；仅操作本次创建的进程，未启动物理 Route。
日志：`test-results/candidate-host-backend.log`。
这未覆盖窗口生命周期、摄像头管理员路径或任何媒体消费者。

## 当前交付边界

本轮主页初始化修复已在上述完整 Android APK 中；13:46 Windows/Core 安装包仍为
前一批，未包含本轮初始化修复。保留独立哈希，不将本轮结果写成两端同批构建。
新版跨设备音频、摄像头、触控板及实验能力回归仍未完成，锁屏/睡眠仍暂缓。
普通权限相机、新机器组件部署、公开许可和生产签名仍待解决。
