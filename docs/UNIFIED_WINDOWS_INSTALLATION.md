# CapyIO 统一 Windows 本地 Alpha 安装

此入口管理应用、后台服务、系统摄像头 COM 注册；安装到 Program Files 的独立版本目录。
安装前从 CapyIO 菜单退出节点。启动安装器后确认 UAC，安装结束从开始菜单启动应用。
配对、账号设置和 Android 权限不变；不自动运行 APK 安装。

## 当前适用范围

本机必须已有健康的 CapyIO 音频驱动、Render APO 与 VHF 触控板驱动。
缺少时安装在系统配置变更前停止，并列明缺失组件。本地现有驱动未形成可供默认安全策略
新机器使用的生产签名交付合同，所以本包不冒充驱动完整安装器，不修改 Secure Boot、
测试签名、证书信任或启动配置。显示与手柄外部组件仍是可选前置条件。

## 状态、升级、回滚

开始菜单提供检查安装状态、回滚上一个版本和恢复安装前系统配置。
升级保持现有服务账户、启动方式、网络及可选显示配置，替换服务和音频工作程序路径，
为安装发起账号配置精确 SID 的音频/相机权限。新服务仅以回环地址作旧接口默认值，
真实 Route 仍需配对和资源授权，不会因安装而开始共享。

配置切换失败自动恢复原服务与摄像头注册。回滚恢复上一版，恢复安装前配置会撤销本安装器
创建的服务/COM 配置，或恢复原实验部署。版本文件保留作恢复缓存，不删除配对数据。
可选显示配置的外部文件不迁移、不删除。此行为不是磁盘彻底清理。

受保护的 `Program Files/CapyIO/installation.json` 记录活动版本与回滚历史。
安装器不会覆盖运行中的 Node；关闭窗口不等于退出 Node。

## 构建与验证

构建当前前端、Desktop（custom-protocol）和 Windows workers / camera MF library 后运行
`scripts/build_unified_windows_installer.ps1`。输出 `target/unified-windows/<build>/`，包含
安装器、哈希清单、原始 payload 和构建结果。`manage_windows_installation.ps1 -Mode Check`
只读检查本机组件。普通用户运行 Install / Rollback / Remove 会进行一次 UAC 提升；
Launch 始终使用原用户身份。

哈希是完整性记录，不是发行者签名。此包不用于公开发布。构建成功不代表安装或新机验收通过。


## 可选的新机器驱动自动安装（ADR 1513）

构建入口现支持 `-SignedDriverBundle <目录>`，将完整、通过清单验证的签名驱动作为可选
payload 接入同一安装流程。未提供时仍执行原有前置检查，不声称包含驱动。
新机器创建两种固定 Root 设备，APO 随音频设备枚举；重启请求返回 3010，并要求手动
重启后继续。健康的现有驱动保持不变，部分损坏安装不自动修复。失败日志及本次创建的
设备清理有独立记录；Driver Store 包和成功安装的驱动不随应用回滚/Remove 删除。

目前没有可用的微软签名输入，因此尚未执行这条真实安装路径。
所需材料、输入模板和验证边界见 [签名交接清单](WINDOWS_DRIVER_SIGNING_HANDOFF.md)。
