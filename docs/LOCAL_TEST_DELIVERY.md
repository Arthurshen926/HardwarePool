# CapyIO 双端本地测试交付包

> 本文保留旧 schema-v2 有限包的用法，不是当前完整包说明。
> 当前完整本地调试包、唯一启动路径及升级/回滚见
> [LOCAL_DEBUG_UPGRADE.md](LOCAL_DEBUG_UPGRADE.md)。ADB 仅用于可选安装/诊断，
> 不是 CapyIO 软件运行或两端数据连接的依赖。

这是用于指定设备的本地测试包，还不是面向普通用户的正式发行版。包中统一
Windows 软件与统一 Android APK 来自同一套项目；驱动、系统服务、第三方组件
和用户配对信息需要保留在原设备上，不包含在包里。

## 打开和校验

1. 将整个 ZIP 解压到一个新目录，保留 EXE 旁的辅助程序。
2. 在该目录打开 PowerShell，运行
   `powershell -NoProfile -File .\verify_integrated_test_package.ps1 -CheckPrerequisites`。
   文件缺失、被修改或多出不属于交付包的文件会导致校验失败。校验通过只能证明
   文件与清单一致，不能代替可信来源、生产签名或实机测试。
3. 用原来的 Windows 配对账号启动目录里的 `capyio-desktop.exe`。源码构建模式
   已内嵌界面，不需要另开开发服务器。如果出现 `127.0.0.1 拒绝连接`，检查是否
   打开了其他目录中的旧开发 EXE。使用现有 VHF 触控板时，目前仍需该账号的
   管理员令牌；切换到另一个管理员账号不会继承原账号的配对凭据。
4. Android 文件位于 `android/CapyIO-debug.apk`。由设备所有者确认覆盖安装。
   包校验不会安装 APK、驱动或服务，也不会自动连接设备。

`manifest.json` 记录每个文件的 SHA-256、APK 版本和签名证书指纹。如果
`buildProvenance` 是 `existing-artifacts-source-not-verified`，它只表示现有
产物的可追踪拷贝，不能声称这些产物已按清单中的打包时 Git 版本重新构建。

## 连接与触控板的最短验证流程

1. 平板启动 CapyIO，允许需要的系统权限，在“本机共享”选择触控板。已有配对
   应继续保留；新设备仍按软件当前提供的邀请导入流程建立配对。
2. 两端选择能互通的网络。自动发现可用时选择已配对设备；手动连接使用平板
   CapyIO 设置页显示的本机控制端口，不能使用 Android 无线调试端口。
3. Windows 的“远程资源”应显示真实平板节点及其资源。认证成功与目录可见
   还不代表触控数据已启动。
4. 申请平板触控板，平板允许后进入全屏触控区。分别验证点击、单指移动、
   双指右键、双指滚动、三指及四指手势。手势行为以 Windows 当前设置为准；
   若平板系统三指截图/分屏等操作抢占输入，需要用户手动关闭相冲突的系统手势。
5. Windows 对应资源应出现“停止使用”；点击后平板退出全屏、资源回到可申请。
   平板也可用五指静止长按两秒退出并停止使用。任一停止路径或断线后，Windows
   都不应留下按住的按钮或触点。

上述每一项必须实际观察后才能记为通过。其他音频、摄像头、手柄、IMU、显示
功能还应各自验证资源启动、真实数据、停止和恢复，不能由触控板通过推断。
部分数据通道仍有实验性独立配置，统一节点认证不等于所有功能已自动配置。

## 更新与故障恢复

- Windows：先在 CapyIO 选择“节点 → 退出节点（停止共享）”，解压新版到新目录后启动。
  关窗只退出界面，后台节点仍在运行。保留旧目录便于切回原软件；不要同时运行多个版本。此包不迁移或删除
  `%LOCALAPPDATA%\CapyIO` 中的配对资料。替换 EXE 不会修复错误账号的 DPAPI 身份。
- Android：覆盖安装要求应用 ID 和签名证书与已安装版本一致。签名不匹配时
  停止安装并用原 debug keystore 重建；不要通过卸载或清数据解决，这会丢失
  节点、配对和设置。版本号降低也可能被系统拒绝，应提供用原签名构建的修复版。
- 连接证书错误：核对原配对账号和同一次邀请，不要通过改 IP 或关闭证书校验
  处理。资源故障先停止该资源，再在同一账号中重新连接和申请。
- 本包没有自动更新、签名更新源或数据降级恢复机制。Windows 安装器的升级/
  卸载、生产 APK 签名、全设备回归、长期运行和依赖许可证/NOTICE 完整性审查
  仍是正式交付前的验收项；根目录 LICENSE 不是所有依赖的许可证清单。

## 开发者生成方法

从仓库根目录运行（替换占位值；只用于 Android 标准 debug keystore）：

```powershell
.\scripts\build_local_delivery_package.ps1 `
  -AndroidSdkRoot '<Android SDK 路径>' `
  -AndroidDebugKeystore '<原 debug.keystore 完整路径>' `
  -ExpectedAndroidSignerSha256 '<预期的64位证书SHA256>'
```

脚本顺序构建共享界面、启用 `tauri/custom-protocol` 的 Windows EXE、固定的
三个辅助程序和统一 debug APK，然后验证 APK 签名及包结构。依赖须预先缓存；
Cargo/Gradle 使用 offline 模式。APK 采用标准 `androiddebugkey` 及标准 debug
口令，不接受生产签名配置。预期证书指纹需由可信旧 APK/已安装 APK 的
`apksigner verify --print-certs` 结果取得；不要用本次未知产物的指纹自行放行。

如果刚完成了这些构建，可加 `-UseExistingArtifacts` 直接封装现有文件（不需要
keystore 参数），但清单会如实标注未验证源码对应关系。默认输出目录和 ZIP
位于 `target/local-delivery/`，不覆盖已有包。`-SkipArchive` 仅跳过 ZIP 压缩。
脚本采用固定文件白名单，不扫描/复制 APK 私钥、配对邀请、用户配置或驱动。

本地回归命令：`powershell -NoProfile -File scripts/test_integrated_test_package.ps1`。
它只使用临时假文件验证清单、损坏/遗漏/额外文件和路径穿越拒绝，不接触设备。
