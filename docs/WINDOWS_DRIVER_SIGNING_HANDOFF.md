# 新机器驱动签名材料与接入清单

更新：2026-10-04。当前用户没有签名包或 Hardware Dev Center 账号。
自动安装代码已接入，但不能因此声称驱动包已具备生产签名或实机验收已完成。

## 需要准备的外部材料

1. 可提交硬件驱动的 Microsoft Partner Center / Hardware Dev Center 组织账号与管理员。
2. 与该账号关联且提交时有效的 EV 代码签名证书；私钥留在你控制的硬件/签名环境中，
   不放入代码库或安装包。提交文件需使用已关联的合适证书签名。
3. 固定版本的 x64 音频基础驱动、音频扩展 INF、Render APO、VHF 触控板完整包，
   包含 INF、SYS/DLL 与对应 CAT，以及源码版本、构建工具版本和许可记录。
4. 根据拟用分发途径完成所需测试和签名提交，取得微软返回的签名包。需要 WHCP/WHQL
   认证或 Windows Update 分发时，另准备对应 HLK 测试及提交材料；本轮不选择或承诺
   某条认证路线一定适用。
5. 保存返回包、提交编号、目标 Windows 版本、每个文件 SHA-256、CAT 的签名证书
   thumbprint，以及一台默认安全策略的干净测试机。签名后不再修改 INF 或二进制。

微软依据：[签名策略](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-)；
[账号关联证书要求](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-reqs)。
正式签名和设备实际加载验收不是同一件事；这里不代替门户审核或 HLK 测试。

## 输入目录合同

一个目录包含 `driver-bundle.json`、`audio/` 与 `touchpad/`。
JSON 模板见 `release/windows-driver-bundle.template.json`，占位哈希和 thumbprint 必须
替换成实际签名包值；模板不是可用驱动包。保留微软返回包的原文件名，按实际文件填写。

四个固定角色为 audio-apo、audio-extension、audio-base、touchpad。
每个角色声明 INF、CAT 和固定的 CAT signer thumbprint；files 列出所有需携带文件的
相对路径和 SHA-256。音频的三个 INF 可以共享同一个 CAT。安装器验证目录边界、哈希、
CAT 本机信任状态及证书 pin，并核对 INF 的 ClassGuid、硬件 ID 和 CatalogFile。
这些早期检查不冒充微软认证：Windows 安装/加载仍执行自己的签名与包成员校验。

拿到包后：

```powershell
./scripts/build_unified_windows_installer.ps1 -SignedDriverBundle 'D:\CapyIO-signed-drivers'
```

未传该参数时，生成的安装包仍要求预装驱动，并标记 signedDriverPayloadIncluded=false。
不存在“先打包未签名驱动，到用户电脑关闭校验”的后门。

## 安装行为及边界

- 仅处理全新驱动环境；已有健康组件不重装，部分安装/异常设备明确转为修复问题。
- 使用 Windows SetupAPI / NewDev 创建固定 Root 设备及绑定包，不携带 DevCon、WDK 或
  签名工具；APO 由音频父设备枚举，不能人工创建一个错误的 APO 根设备。
- 不强制降级；不修改启动设置、不导入证书、不自动重启。需要重启返回 3010，用户重启后
  重新运行同一安装器，健康检查通过才继续服务和摄像头配置。
- 每步写入受保护版本目录的 driver-install-*.json。失败只移除本次创建的确切 Root 实例。
  已进入 Driver Store 的包保留并记录，不自动删除可能共享的包。
- 驱动步骤通过后若后续服务/COM 安装失败，驱动仍保留，应用配置按原事务回滚。
  Remove/应用回滚不卸载驱动；驱动升级、修复、卸载需要独立精确授权和验收。
- 本轮只执行编译及使用假签名返回值的清单测试，没有调用真实驱动安装/删除接口。
  驱动加载、APO 枚举、重启续装和失败后的硬件恢复仍需要签名包与实机验收。
