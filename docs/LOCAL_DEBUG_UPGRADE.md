# CapyIO 完整本地调试包：使用、升级与回滚

此页描述 Integrated 包；独立 Core 文件包见 [Core 说明](CORE_ALPHA_PACKAGE.md)。
Core 与 Integrated 不支持运行文件原地互换，请使用新的应用目录。

此包不是生产签名安装器，也未获准公开分发。包含 Android 完整统一 debug
APK（含显示接收 JNI）、Windows 桌面、音频服务/工作程序、显示工作程序与
明确列出的显示/Xbox 依赖。驱动、系统相机注册、USB/IP/VIIPER 部署、Broker
服务配置仍须保留原安装；复制文件不等于安装或验证这些组件。

## 唯一应用入口

### 本地测试安装向导（未签名）

维护者可用 `scripts/build_local_debug_installer.ps1 -PackageRoot <完整包目录>`
调用已有 Tauri NSIS 工具生成 setup.exe，不自动下载工具。向导只安装当前用户的
应用文件到新的版本目录，并创建「CapyIO Local Debug」开始菜单快捷方式；不自动
启动程序，不申请提权，不部署服务、驱动或 Android APK。旧版本与配对数据保留。
已有目标目录会拒绝覆盖；更新采用并排版本，服务路径不会自动迁移。

此向导尚非生产签名或完整系统安装器，不能声称新电脑安装后所有硬件立即可用。
可使用 `remove_local_debug_bundle.ps1 -TargetDirectory '<应用目录>'` 预览卸载，
确认后加 `-Apply`。该入口将完整应用目录移到同级 `.removed-<标识>` 备份，并仅移走
指向该版本的开始菜单快捷方式，不删除 AppData 中的 Control/Pairing 数据。
目录被服务引用、程序仍运行或包已被改动时拒绝处理；无法读取服务列表也不会继续。
这是一种可恢复的应用移除，不回收磁盘空间、不卸载驱动/服务。恢复时以备份目录作为
install_local_debug_bundle.ps1 的 PackageRoot，安装回原目录；快捷方式需重新建立。
包校验及命令行安装入口支持 Windows 自带的 PowerShell 5.1，无需额外安装 PowerShell 7。

### 安装应用文件到独立目录

在解压后的包目录运行（目标必须是尚不存在的新目录，其父目录已存在）：

```powershell
.\install_local_debug_bundle.ps1 -TargetDirectory 'C:\CapyIO-local-vNext'
# 核对预览后执行：
.\install_local_debug_bundle.ps1 -TargetDirectory 'C:\CapyIO-local-vNext' -Apply
```

脚本校验源包及复制后的文件，不启动程序，不覆盖已有目录，不修改配对或系统组件。
复制失败会保留新目录供排查，不影响原安装。切换版本前选择旧程序“节点 → 退出节点（停止共享）”，再用
原 Windows 账号打开新目录中的 EXE。服务仍指向原配置，不会随文件安装自动迁移。
这只是本地应用文件安装入口，不是已签名的完整系统安装器。

新电脑先运行 `./check_local_prerequisites.ps1` 获取只读组件报告。报告区分服务状态、
设备是否被发现及无法读取，不自动安装组件。相机注册、DS4/VIIPER 必须通过应用检查；
设备名称匹配不等于硬件可用。当前包不能替代完整驱动和服务安装器。

先在旧 CapyIO 选择“节点 → 退出节点（停止共享）”，在包目录运行：

```powershell
.\verify_local_debug_bundle.ps1 -PackageRoot .
.\capyio-desktop.exe
```

关闭窗口只退出 UI，节点仍在后台运行；重开界面接回原节点。升级、回滚、移除前
必须选择“退出节点”，仅关窗不够。新目录的 UI 不会连接旧目录的 Host。

只启动此 `capyio-desktop.exe`，不要手动启动服务/worker EXE。界面已内嵌，
不需要 localhost 开发服务器。使用原 Windows 配对账号；如既有驱动要求
管理员令牌，仍使用同一个账号确认 UAC，不能换账号绕过 DPAPI。

节点认证和媒体走两端实际可达的局域网/Tailscale 网络。ADB **不是运行依赖**，
无线调试端口不是 CapyIO 控制端口。ADB 仅可用于用户授权后的覆盖安装或诊断，
不是节点连接隧道。名字含 `adb-lab` 的触控辅助程序仍保留兼容文件名。

`manifest.json` 包含每个文件的大小、SHA-256、源码/打包来源与 APK 签名指纹。
校验只能确认字节一致，不能证明发布者身份或所有硬件都验收通过。

## Windows 本地文件升级

### 屏幕共享使用与恢复

1. 两端使用原配对身份连接。若从平板发起，先在 Windows「本机共享」
   勾选「Windows 扩展屏幕」；已配对不等于已发布显示资源。
2. 在「跨设备扩展屏」选择节点和“自动”画质。首次资源请求仍需允许；
   自动模式在启动时选择屏幕比例、最高 60 FPS 和推荐码率，不是网络动态码率。
3. 任意一端停止：平板点“停止共享”，或 Windows 点“停止共享并恢复桌面”。
   等待停止完成后再开启下一次；不要单独启动 worker。
4. 平板按 Home 或锁屏会停止本次显示共享；音频等其它共享不由此操作停止。
   回到前台后重新开始，不自动恢复屏幕会话。旋转由接收页适配，不切换协商参数。
5. 网络断开后应退出接收并清理扩展屏。恢复节点连接后重新开始。
   若提示“停止/恢复未确认”，先检查显示服务和 Windows 显示布局，不视为已恢复成功。
6. 不要在共享中直接把 Windows 扩展屏改成 120 Hz；当前产品界面只支持
   30/60 FPS，模式被外部修改会中止本次共享。先停止，再在 CapyIO 调整画质。

接收页统计中的“协商 FPS/码率”是配置，不是实测吞吐；“实测呈现”来自帧计数。
故障时保留页面错误码；不要通过重配对、卸载或清数据处理普通显示故障。

显示功能本阶段按用户确认收尾，沿用此前出画、持续运行及停止恢复的验收。
2026-10-01 的 RDP 环境绑定失败不作为新的通过证据；用户明确不再保留单独补测任务。

### 首次连接与更新后恢复

1. 首次添加优先使用「设置 → 添加新设备 · 无文件配对」：任一端显示 PIN，
   另一端输入，核对设备并在双方确认。文件邀请仅保留为兼容入口。
2. Tailscale 不保证多播发现；首次搜索不到时，填写对方 Tailscale IP 和界面显示的
   配对端口。配对端口、控制端口、无线调试端口用途不同，不要互换。
3. 已信任设备更新后直接连接，不必重复配对。自动连接会尝试已保存的认证地址；
   地址变化时在设置中更新。信任节点不等于开放所有硬件，仍须发布及授权资源。
4. 超时先检查网络，端口拒绝先检查对端 CapyIO 是否运行和监听；不要清数据。
   证书不一致才检查双方配对批次；重复输入 PIN 不会覆盖已有证书。
5. Android 当前无文件首配仍有单个已配对节点限制，不承诺任意多节点首次配对。

升级程序默认只打印计划。明确目标为一个专用应用目录，关闭其中运行的所有
CapyIO 程序，然后：

```powershell
.\update_local_debug_bundle.ps1 -PackageRoot . -TargetDirectory 'C:\CapyIO-local'
# 核对计划后，才执行文件替换：
.\update_local_debug_bundle.ps1 -PackageRoot . -TargetDirectory 'C:\CapyIO-local' -Apply
```

目标目录应预先创建。脚本只替换清单中的根目录 EXE/DLL；不读取/覆盖 AppData
中的配对、证书、设置，不复制私钥/邀请，不启动程序，不安装 APK、驱动或服务。
若目标包含已注册的 Broker，脚本拒绝执行：服务工作程序路径/哈希配置必须通过
另行批准的服务更新流程同步，不能用此文件升级脚本绕过服务边界。

完整 schema-v3 安装目录升级时同步清单中的运行文件哈希，保留原 APK/文档记录，
另记 runtimeUpgrade 来源。清单与运行文件一起回滚，不把局部升级标成整包重建。
校验已升级目录时使用 `verify_local_debug_bundle.ps1 -PackageRoot <目录> -AllowRollbackBackups`；
默认分发包校验仍拒绝备份和其他额外文件。可恢复移除支持已升级目录，恢复时保留
原归档中的回滚历史，只复制应用清单所列文件到新目录。

此运行文件升级会保留目标原有的 `.ps1` 工具。校验、回滚、移除和恢复应继续使用
新分发包中的工具路径；本节的相对命令均在新分发包目录执行，不调用旧安装目录
中的脚本。旧校验器可能不支持 `-AllowRollbackBackups`。

旧文件备份到目标目录 `.capyio-backups/<时间戳>`。复制失败自动尝试恢复；
如果文件占用导致恢复也失败，关闭占用程序并用保留的备份重试，不删除配对：

```powershell
.\update_local_debug_bundle.ps1 -Mode Rollback -TargetDirectory 'C:\CapyIO-local' -BackupName '<升级输出的时间戳>'
.\update_local_debug_bundle.ps1 -Mode Rollback -TargetDirectory 'C:\CapyIO-local' -BackupName '<升级输出的时间戳>' -Apply
```

回滚会校验原文件哈希。旧版本不存在的新文件移入备份目录保留，不做递归删除。
本流程只回滚程序字节，不承诺任意版本配置格式向后兼容；这一阶段没有生产级
数据迁移/系统安装器回滚。服务部署和运行中的共享应先单独停止。

## Android 更新

由设备所有者确认覆盖安装 `android/CapyIO-debug.apk`，签名必须与旧版相同。
不要卸载或清数据。脚本要求从可信旧 APK/已安装 APK 获取的签名 SHA-256，
不会生成或替换签名身份。版本号降低被系统拒绝时，不清数据强行降级；用原签名
构建更高 versionCode 的修复版。Android 的配对、权限与系统设置均不自动修改。

## 生成完整包（维护者）

```powershell
.\scripts\build_integrated_test_package.ps1 `
  -ExpectedAndroidSignerSha256 '<可信旧APK的64位证书指纹>' `
  -AndroidSdkRoot '<SDK目录>' `
  -DisplayRuntimeManifest '<显示依赖目录中的package-inputs.json>' `
  -AcknowledgeLocalDebugThirdParty
```

脚本默认离线重建第一方产物和含显示 JNI 的完整 APK；显示第三方 stage 及
Xbox sidecar 必须已按现有记录准备，不隐式下载/重新构建。显式依赖清单格式：

```json
{
  "distribution": "local-debug-only",
  "licenseNotice": "GPL display integration; local debug only; upstream/patch records remain required before any redistribution",
  "artifacts": [
    { "file": "libcapyio_display_engine.dll", "sha256": "<64位SHA256>" },
    { "file": "tools/sunshine_wgc_capture.exe", "sha256": "<64位SHA256>" },
    { "file": "libgcc_s_seh-1.dll", "sha256": "<64位SHA256>" },
    { "file": "libstdc++-6.dll", "sha256": "<64位SHA256>" },
    { "file": "libvpl-2.dll", "sha256": "<64位SHA256>" },
    { "file": "libwinpthread-1.dll", "sha256": "<64位SHA256>" }
  ]
}
```

清单旁的文件逐个校验；不会扫描复制整目录（避免误带连接配置/凭据）。显示
worker 固定取本轮 `target/debug/capyio-display-engine-worker.exe`，不取第三方
stage 中可能过期的 worker。APK 缺少显示 JNI 会直接失败，不交付残缺 APK。

如果用 `-UseExistingArtifacts`，只封装已经构建的文件，清单明确记录源码对应
关系未验证；打包前停止并行构建。输出为新建 `target/integrated-test/CapyIO-debug-*`
目录，不覆盖旧包、不生成公开发布 ZIP、不推送远端。

需要公开交付时仍须完成依赖许可证/NOTICE/对应源码义务、生产签名、安装器、
升级迁移和当前设备实机回归；此本地 gate 不等于这些事项已经完成。
