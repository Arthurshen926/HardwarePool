# 本地 Alpha 候选交付（2026-10-03）

本次目标由用户确认为“可安装的本地 Alpha 候选”，不是公开正式发布。
包含最近的界面整理、媒体面板、无障碍文案及统一停止按钮。

## 使用范围

- Windows 当前用户的应用文件安装，保留配对和设置。
- 包内 Android APK 使用原 debug 签名身份，但安装到个人设备仍是单独操作。
- Core 不包含实验显示引擎和 ViGEm 后端；Extras 单独留存，不自动覆盖 Core。
- 音频/VHF 驱动、Broker 服务和相机注册仍依赖本机已部署环境，安装器不部署它们。
- 使用原已配对的 Windows 账号；切换版本前通过“节点 → 退出节点”结束旧 Host，
  仅关窗不会退出 Host。摄像头当前仍需要管理员 Host。

## 验证边界

当前源码通过完整 Alpha 软件 Gate：Rust 1116 passed、0 failed、31 ignored，
前端 89 项检查、Python 10 项交付工具检查，Android 92 项任务完成。
原生日志保留常规 MSVC linker_messages 提示，不把忽略项计为通过。

固定源码候选：`target/source-export/20261003-103637-972474`，1724 个文件。
第一次独立构建中 cargo_metadata 编译进程退出，未提供具体诊断；保留失败日志，
随后限制 CARGO_BUILD_JOBS=4，在全新目录重试，不混用失败批次产物。

依赖证据：`target/release-evidence/local-candidate-20261003`，674 个组件记录、
901 份许可文本，117 项许可证据不完整。这是构建前工作区清单，包含非分发依赖，
不能作为最终二进制许可审查结论。源码扫描 356 项中 10 项现有说明仍匹配哈希，
346 项待审；近期界面修改使部分旧说明自动失效，未自动沿用。

当前包的实机媒体回归尚未执行；此前 Y700 验收不能自动覆盖新哈希。
锁屏/睡眠依用户要求继续暂缓。公开签名、许可证/源码清理及新机器部署验收
仍未完成，publicationApproved 保持 false。

日志：`test-results/release-candidate-alpha-check.log`、
`release-candidate-source-build.log`、`release-candidate-source-build-retry.log`、
`release-candidate-installer-regression.log`、`release-candidate-license-inventory.log`。


## 本次交付文件与结果

目录：`target/alpha-profiles/20261003-104258-232954`。

- Windows：`CapyIO-local-20261003-104338-6463-setup.exe`。
- Android：`Core/android/CapyIO-debug.apk`，独立源码构建与签名验证通过。
- `Core.zip` 为同批免安装文件；Experimental-Extras.zip 是独立的旧实验依赖归档，
  不作为默认安装内容，不建议为日常使用直接覆盖 Core。
- `checksums.json` 包含安装器与两份 ZIP 的 SHA-256；`source-build-result.json`
  绑定本批 8 个 Windows 程序和 APK，六个构建步骤全部通过，源码输入未变。

安装器 SHA-256：`7ba3a0244861d6dcbc796f537fc7434847f3d080231810dd017ed794b1a5a74e`。
APK SHA-256：`26d8056d8c27fb69f7a484fe82459cba92835159f77e931fd8304410065c096c`。

本批真实安装器已在候选目录内的 installation-check/application 完成无快捷方式
文件安装、升级、精确清单回滚、可恢复移除及恢复；未启动应用、注册服务、部署
驱动或安装 APK。证据：`test-results/release-candidate-file-lifecycle.log`。

本地候选交付可以收口；实机回归、公开分发和普通权限摄像头仍不在通过范围内。
