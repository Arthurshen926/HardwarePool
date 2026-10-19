# 云端源码同步交接（2026-10-19）

分支：`codex/cloud-sync-sanitized-20261019`。
公开基线：`b42092b55c3dcaa2581f5dfa077cd39bc749c1c7`。

这是用户批准的当前源码快照，不继承本地原分支的 94 个未推送提交。
原分支、暂存区和原工作文件保持不变。固定输入为 1,750 个源码、配置、许可证和
开发文档文件，已记录逐文件 SHA-256；本交接文件另行加入。原始清单和清理审计
只保留于本地隔离目录，不随源码推送。

## 清理与边界

个人目录、账户、主机名、真实用户 SID、实验网络及运行时 Node/Route/端点标识
改为显式占位；历史配对时间和证书身份信息也已匿名化。`LAB-WINDOWS-HOST`、
`<LAB_IPV4_XX>`、`<LAB_ID_XX>`、`<approved-user-sid>` 不代表任何已获批目标。
实际实验必须重新核实具体目标并单独获批。

标准网络范围、公开 COM/HID 常量、第三方补丁上下文和明确的合成测试数据保留，
以维持协议、地址准入和测试语义。控制器独立实验 App 的预填电脑地址改为空，
由操作者填写。文本 LF 已修复，Windows BAT 保持仓库规定的 CRLF。

未纳入 Git 私有历史、依赖/编译缓存、实验输出、个人配置、凭据、独立代理私人提示
和已编译的 EXE/DLL。保留的 52 个图标及 Gradle wrapper 二进制输入均与公开基线相同。
没有增加能力、权限或部署行为，也未将历史验收结果当作本快照的新实机结果。

## 本次验证

- `python scripts/validate_repository.py`：通过，88 个 Requirement ID。
- `python -m unittest discover -s scripts -p "test_*.py"`：11 项通过。
- `cargo fmt --all --check`：通过。
- `cargo metadata --no-deps --offline --locked --format-version 1`：52 个 package。
- `cargo run --offline --locked -p xtask -- validate-manifests`：2 个 manifest 通过。
- `scripts/*.ps1` PowerShell AST 解析：78 个文件通过。

未运行完整 `alpha-check`、原生/前端/Android 全量构建或硬件回归，没有设备安装、
签名、部署或安全设置修改。源码同步不构成正式发布或许可材料闭合；原有产物流程
中的 `publicationApproved=false` 保留。模式扫描及上下文核查也不构成安全认证。

## 云端入口与待办

```sh
git fetch origin codex/cloud-sync-sanitized-20261019
git switch --track origin/codex/cloud-sync-sanitized-20261019
```

先读根 `AGENTS.md` 和对应目录规则。本次代码入口是上述分支。

- [当前状态](CURRENT_STATUS.md) 与 [五阶段 Alpha 计划](ALPHA_CONSOLIDATION.md)
- [统一 Windows 交付](UNIFIED_WINDOWS_DELIVERY_20261004.md)
- [安装与恢复语义](UNIFIED_WINDOWS_INSTALLATION.md)
- [签名驱动材料与待办](WINDOWS_DRIVER_SIGNING_HANDOFF.md)
- [测试策略](TESTING.md)

既有优先项是完整软件 Gate、当前候选实机回归、新机器组件安装/升级/回滚验收，
以及生产签名和实际分发依赖材料收敛。锁屏/睡眠和实验能力恢复矩阵保留既有边界。
本次只同步源码，没有推进这些开发或部署事项。
