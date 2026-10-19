# 统一 Windows 安装交付记录（2026-10-04）

## 当前产物

`target/unified-windows/alpha-20261004-040244-9197/CapyIO-alpha-20261004-040244-9197-setup.exe`

SHA-256：`5CE9D027ADC3E63849BDFF7BCCF909FE11BDE1E1F7BA4127B8938287484AF147`。
大小 7312853 bytes。构建自当前工作区的前端、Desktop 和 Windows helpers；源码有既有未提交改动。
本地 debug 产物，不是生产签名或公开发布。

统一入口：`scripts/manage_windows_installation.ps1`；安装器源码
`scripts/unified_windows_installer.nsi`，构建入口 `scripts/build_unified_windows_installer.ps1`。
范围及恢复语义见 `UNIFIED_WINDOWS_INSTALLATION.md`、ADR 1512。

## 已验证

- Vue 构建、嵌入 UI 的 Desktop 构建、相关 Windows binaries/library 构建通过。
- NSIS 中文安装器生成通过；首次因未声明 UTF-8 BOM 编码失败，已修复重建。
- 13 项 payload 哈希核对通过。
- PowerShell 语法、既有配置恢复、新建服务恢复、包路径越界拒绝检查通过。
  恢复检查使用显式假的系统操作，不能冒充 SCM/COM 实际回滚验收。
- 本机只读预检：音频、Render APO、VHF 三项现有设备健康；服务运行，摄像头 COM 已注册。
- 文档 traceability 检查通过（88 Requirement IDs）。

证据在 `test-results/unified-installer-*.log`、`test-results/unified-installation-preflight.json`。

## 待验收与限制

已向用户提交上述具体安装包的本机安装、回滚、重新安装确认，尚未执行系统修改。
不能据构建和模拟检查声称实际安装、回滚已通过。

此包统一管理应用、服务及摄像头 COM，保留配对和原配置，升级保留外部可选显示配置。
恢复原系统配置保留版本目录作为恢复缓存，不是彻底删除所有文件的卸载。
驱动仍是前置条件：缺失/异常时完整 Core 安装在系统配置修改前中止。没有驱动安装、
测试证书导入或启动安全策略更改。无可默认信任的新机驱动交付输入，干净机器完整安装未完成。

## 2026-10-04 驱动自动安装代码接入

用户明确选择“没有签名包，先完成代码并列明签名材料”。因此本轮不执行驱动安装、卸载、
签名或系统安全设置变更，之前的具体旧安装包执行确认不自动套用到新包。

新增 `scripts/windows_driver_setup.cs`（原生 SetupAPI/NewDev 互操作）和
`scripts/windows_driver_payload.ps1`（包验证、新机安装编排、设备清理日志），统一入口与
构建脚本已接入可选 `-SignedDriverBundle`。合同见 ADR 1513，材料清单见
[WINDOWS_DRIVER_SIGNING_HANDOFF.md](WINDOWS_DRIVER_SIGNING_HANDOFF.md)。

当前无签名输入的新版安装器：
`target/unified-windows/alpha-20261004-043411-1117/CapyIO-alpha-20261004-043411-1117-setup.exe`

SHA-256：`EC3741F255E41457B1D970C3AE755874CE8E8E12D0E6E1643BD3B0161CC540AE`。
`driverInstallationIncluded=false`，`installed=false`，`newMachineAcceptance=false`。
无驱动输入时仍要求现有健康驱动；不能把自动安装代码与已签名的完整驱动交付混为一谈。

验证：PowerShell 语法、Windows PowerShell 5.1 中 C# 互操作编译、使用假签名返回值的
完整清单/未签名拒绝/哈希篡改拒绝检查、既有安装器恢复检查、NSIS 构建、88 个需求文档
校验通过。原生驱动安装/卸载 API 均未调用。日志为
`test-results/windows-driver-payload-tests.log`、`unified-driver-capable-installer-build.log`、
`driver-installer-docs.log`。签名包接入后的加载、APO 枚举、重启续装与清理仍需实机验证。
