# 本地交付证据生成

本入口补齐阶段 4 的构建前材料，不授权公开发布，不操作设备或生产签名。
依赖现有 Python 3.11+、PyYAML、Cargo、pnpm 安装目录和离线 Gradle 缓存；不自动安装工具。

从仓库根目录执行：

```powershell
Push-Location platform/android
./gradlew.bat --offline --no-daemon -I ../../scripts/release_inventory.gradle capyioReleaseInventory
Pop-Location
python scripts/build_release_inventory.py --output target/release-evidence/<新的目录名>
```

输出目录必须不存在，避免覆盖已保存的证据。

- `sbom.cdx.json`：按 [CycloneDX 1.5 JSON](https://cyclonedx.org/docs/1.5/json/)
  生成的 pre-build 清单，含 Cargo 解析图、pnpm 锁定依赖、Android debugRuntimeClasspath
  模块和第三方集成声明。包含开发依赖、其他平台依赖及尚未确认装包的集成记录。
- `license-index.json` 与 `licenses/`：上游声明和本地可找到的原始 LICENSE/COPYING/
  NOTICE 文本。工作区 Apache-2.0 包使用仓库 LICENSE。文本按组件保留，可能重复；
  不推断许可证兼容性，也不将缺失文本替换成猜测内容。
- `evidence.json`：源码 revision/dirty、输入哈希、数量、缺失项和覆盖范围。
- `android-runtime.json`、`third-party-records.json`：解析依据与已有固定版本/修改记录。

脚本验证组件引用唯一且依赖引用闭合。本轮没有引入新的 schema 验证依赖；未声称
通过官方 JSON Schema 验证或完成许可证法律审查。Gradle 插件/构建工具、Android
平台和外部原生/JNI 的传递依赖不在自动扫描范围。公开包仍须把实际二进制清单、
对应源码/补丁/构建配方与此清单逐项对应，不能把 pre-build 清单作为精确装包证明。

## 本机仅文件安装验收

```powershell
./scripts/test_local_debug_installer.ps1
```

复用既有包测试的假文件，编译现有 NSIS 安装器，在工作区 `target/installer-check-*`
执行无快捷方式安装，检查拒绝覆盖、可恢复移除及还原。不会运行假 EXE，不部署
驱动/服务/APK，不改配对或设置，不删除测试目录。此检查需要已有 NSIS；不下载。
安装界面明确提示：升级前使用“节点 → 退出节点”，仅关窗会保留后台 Host。

2026-10-02 本机结果：674 个组件/集成记录、901 份许可证文本（含重复许可），
117 项许可证证据不完整：Cargo 54、npm 43、外部集成 18、Maven 2。
这里的缺失数包括非当前平台依赖；它不是实际分发包的违规数量或法律结论。
仅文件 NSIS 安装、覆盖拒绝、升级/回滚、移除/还原检查通过。
日志为 `test-results/alpha-file-installer-check.log` 和 `release-android-inventory.log`。

实机长测、系统组件安装、生产签名、最终二进制许可证/对应源码验收仍未通过；
`release/public-alpha.json` 的 `publicationApproved` 保持 false。

## Android 实验显示的原生输入证据

默认 `capyioCheck` 不含可选 GameStream 库。已有本地原生 stage 时，单独执行：

```powershell
Push-Location platform/android
./gradlew.bat --offline --no-daemon :app:verifyGameStreamDebugApk -PcapyioGameStreamStageRoot=target/android-moonlight-unified
Pop-Location
python scripts/build_native_source_evidence.py --stage target/android-moonlight-unified --apk platform/android/app/build/outputs/apk/debug/app-debug.apk --output target/release-evidence/<新的原生证据目录>
```

此命令会重建本地 debug APK，但不安装它。后续不带 stage 参数的默认构建可能再次
生成不含显示库的 APK；证据只绑定记录中的 APK SHA-256，不代表同路径未来文件。

本轮 `target/release-evidence/native-inputs-20261002/` 保存记录输入 ZIP 与 evidence.json。
验证 269 个源码文件、2 个集成文件、补丁、编译命令以及 APK 内 ARM64 库的哈希。
APK SHA-256 为 `8411d0f5e39be10bdf1b0e17324831c891c926affbf0c578f12e0a4d583f51d0`。
构建与核验日志为 `test-results/native-display-apk-check.log`、`native-source-evidence.log`。

这不是独立可重复构建证明。OpenSSL 现有输入是头文件/预编译静态库，仍缺完整对应
源码及其交叉构建配方；Windows 显示引擎也仍须单独建立二进制到源码的对应材料。
脚本将这些缺口写入证据，不把归档存在当作公开分发许可。

## 2026-10-03 当前安装 APK 与源码补充

当前已安装 APK 的原生输入重新绑定到 SHA-256
`af7eb79f0697e93a59f46da1ad38be8c825c9c44f6e1ccf200849a720e4e9d00`。
`target/release-evidence/native-source-closure-20261003/` 还包含分发计划固定哈希的
完整 OpenSSL 4.0.2 ZIP，以及从固定 Moonlight Android 归档提取的原上游
`build-openssl.sh`。命令为：

```powershell
python scripts/build_native_source_evidence.py --stage target/android-moonlight-unified --apk platform/android/app/build/outputs/apk/debug/app-debug.apk --openssl-source target/release-evidence/openssl-4.0.2-source.zip --output target/release-evidence/native-source-closure-20261003
```

源码 ZIP SHA-256：`73b9138088c21c3dca17efca874f171684027bb87a3ef083598aa313a309ade7`；
证据归档 SHA-256：`bb251ca8c88a69b03459ad77fe84cdbe730db4139143c03dc0b08f19ceb4091f`。
该补充关闭“没有完整 OpenSSL 源码及上游配方文件”的材料缺口，未证明旧预编译
静态库的原编译器版本或独立重建一致性；Windows 显示引擎分发材料仍独立待验收。

本轮还修复 schema-v3 已安装目录的运行文件升级清单漂移：清单随运行文件备份/
回滚，已安装目录可显式允许回滚备份，分发包校验仍拒绝额外目录。完整安装、
升级、校验、清单精确回滚、可恢复移除及恢复均在 PowerShell 7 与 Windows
PowerShell 5.1 通过现有文件测试。日志为 `alpha-followup-bundle.log` 与
`alpha-followup-bundle-ps51.log`；使用假载荷，未部署系统组件。
