# 本地 Alpha 体验收口候选（2026-10-03）

最新本地候选目录：`target/alpha-profiles/20261003-134603-878363`。替代 13:13 候选作为当前试用入口，未公开发布。

## 包含的改进

- 设置页以 PIN 首配为主，手动连接和诊断折叠，运行/异常相关入口仍可见。
- 常用状态与操作说明中文化；按手柄模式解释用途及所需条件。
- 音频、摄像头、显示区分加载中、读取失败与空目录，提供重试及折叠技术原因。
- 配对与媒体操作防止旧查询覆盖新状态，离开页面不再处理迟到响应。
- 手柄/显示详情按目标初始化，避免沿用上一个目标的选择；活动会话服从后台实际状态。
- 显示组件未加载/服务不可用时前置提示，旧版显示活动会话自动展开停止入口。
- 保留关窗后台运行、独立停止和原有授权规则。没有新增依赖、驱动操作或硬件功能。

## 文件

Windows：`CapyIO-local-20261003-134628-8937-setup.exe`。
APK：`Core/android/CapyIO-debug.apk`。
免安装：`Core.zip`。`Experimental-Extras.zip` 仍为旧实验依赖归档，不自动覆盖 Core。

安装器 SHA-256：`75d3542b93aa39d11b1c4cdf4104573000c41b05ebb36c203da01c9e683672d2`。
APK SHA-256：`92c7f862148f16b9daf816537afa11f8576871d162a34e5934c1f4c05dab4345`。
`checksums.json` 和 `source-build-result.json` 保留完整产物绑定。

升级前用“节点 → 退出节点”结束旧 Host，仅关窗不退出。安装器只安装应用文件，
系统驱动、服务和相机注册仍为前置条件。摄像头需要管理员 Host；APK 使用原 debug
签名身份，本轮未安装到个人设备。安装顺序见 [Windows 入门](FIRST_RUN_WINDOWS.md)。

## 验证

- 前端 92 项检查、类型检查和生产构建通过；仓库结构/diff 检查通过。
- 固定源码候选 `target/source-export/20261003-134237-232908`，1727 文件。
- 独立构建 `target/source-build/20261003-134308-1912/result.json` 六步骤通过，源码
  输入未变，Android 40 构建任务完成并验证签名；Windows 八个程序与 APK 来自同批。
- Core 16 文件、Extras 60 文件完整性通过。真实安装器在本批
  `installation-check/application` 无快捷方式安装，安装后 16 文件校验通过。
- 浏览器演示确认手柄模式说明随选择更新、显示无设备时禁止启动及诊断默认折叠。
  这些检查没有启动实际硬件。此前 390px 设置页验证仍见当前状态记录。

日志：`test-results/ux-closeout-regression.log`、`ux-closeout-build.log`、
`ux-closeout-package.log`、`ux-closeout-installer.log`、`ux-closeout-install-check.log`。

## 未完成与适用范围

这是一轮界面与本地交付收口，不是完整产品验收。最新候选未进行媒体实机回归，
未重复原生全量测试、Android 完整 lint Gate 或升级/回滚矩阵；旧记录不自动覆盖新包。
普通权限摄像头、新机器系统组件部署、完整兼容与故障恢复仍待完成，锁屏/睡眠按用户
要求暂缓。主界面更简洁，技术诊断仍保留部分协议名、原始状态码和英文标识。

公开源码/许可审查及生产签名尚未完成，`publicationApproved=false`。Extras 复用来源、
可信 LAN/Tailscale 限制及媒体并非统一应用层加密的边界不变。不能宣称已满足所有用户、
游戏和设备的完整兼容或已经可以公开稳定发布。
