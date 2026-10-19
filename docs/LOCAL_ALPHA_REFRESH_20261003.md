# 本地 Alpha 更新候选（2026-10-03）

本批替代此前 10:42 批次作为最新本地候选，未公开发布。

## 改动

- 配对、音频、摄像头、扩展屏面板修复命令与轮询结果乱序；页面卸载后不再处理迟到结果。
- 页面退出仍不停止后台资源，配对协议与信任规则不变。
- 只读预检分别检查麦克风和扬声器，区分设备报错、健康未知和存在，配对标为未检查。
- 当前状态与首次使用说明统一到当前原生路径。

## 交付

目录：`target/alpha-profiles/20261003-131348-195829`。

- Windows：`CapyIO-local-20261003-131406-8656-setup.exe`。
- Android：`Core/android/CapyIO-debug.apk`。
- 免安装：`Core.zip`；实验依赖仍单独置于 `Experimental-Extras.zip`，不自动覆盖 Core。
- `checksums.json` 记录安装器与 ZIP 哈希；`source-build-result.json` 绑定八个 Windows 程序和 APK。

安装器 SHA-256：`af14a2c3591c7a2b0086a275e1be48f4653aded5d9641049a050bee2579bf122`。
APK SHA-256：`22c38dc28a0b92bb54b36020a3eacf573142e0383cf2610918aabc6da4ac5603`。

升级前用“节点 → 退出节点”结束旧 Host，仅关窗不会退出。安装器只复制应用文件，
不部署驱动、服务或相机注册。使用原配对 Windows 账号；摄像头仍需要管理员 Host。
Android APK 使用原 debug 签名身份，未安装到平板。具体流程见
[Windows 入门](FIRST_RUN_WINDOWS.md)。

## 验证与限制

当前工作区全量前端 91 项、Python 交付工具 10 项检查通过；类型检查、前端构建、
仓库结构与 diff 检查通过。本轮未重跑 Rust 全量测试和 Android 完整 lint Gate；
此前全量软件 Gate 的范围见 [前批记录](LOCAL_ALPHA_CANDIDATE_20261003.md)。

源码候选 `target/source-export/20261003-130441-930892` 共 1726 个文件。
首轮 Android NDK clang 在受限环境返回 Permission denied，保留失败报告；随后在
允许工具执行的环境从新目录重建。成功报告为
`target/source-build/20261003-131049-3262/result.json`：六步骤通过，Android 40 项
构建任务完成、签名验证通过、源码输入未变化。复用依赖缓存，不声称密闭可重现构建。

Core 16 文件及 Extras 60 文件完整性通过。真实安装器已在本批
`installation-check/application` 独立目录执行无快捷方式安装，安装后 16 文件
完整性通过。没有启动应用、修改正在使用的安装或安装 APK。本批未重复升级/回滚矩阵。

本批媒体实机回归尚未执行；锁屏/睡眠继续按用户要求暂缓。普通权限摄像头、新机器
系统组件部署、生产签名和实际分发许可证审查仍未完成。源码扫描 356 项中 10 项
说明匹配、346 项待审。Extras 仍复用历史产物。`publicationApproved=false`。

证据：`test-results/ui-consolidation-regression.log`、`refresh-candidate-build.log`、
`refresh-candidate-build-retry.log`、`refresh-candidate-package.log`、
`refresh-candidate-installer.log`、`refresh-candidate-install-check.log`。
