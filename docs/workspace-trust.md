# iPad workspace 信任修复

补丁：[`patches/ios-workspace-trust.patch`](../patches/ios-workspace-trust.patch)，
对应固定上游提交 `3440251b30d5c5b522d03be285ab794dcb96bcd5`。

## 原因与修复

iOS 手工初始化应用时没有初始化 `TrustedWorktrees`，使远程项目没有注册信任跟踪，
workspace 也没有订阅信任持久化事件。远端需要信任时，客户端无法正常处理
`RestrictWorktrees` / `TrustWorktrees`，语言服务器可能一直等待。
此外，iPad 没有安装桌面标题栏，因此缺少桌面版的 Restricted Mode 按钮。

- 在设置 `AppDatabase` 后读取 `WorkspaceDb::fetch_trusted_worktrees` 并初始化信任系统，
  早于项目与 workspace 的创建。读取失败会记录错误，并从空的信任列表开始。
- 通过 workspace 创建观察者添加状态栏按钮，覆盖新连接与重建的 workspace。
  按钮读取当前 worktree store 的信任状态，只响应本项目的信任事件；移除 worktree
  时也更新显示。
- 当前 workspace 首次需要信任时自动打开现有 `SecurityModal`。后台 workspace
  不抢焦点，已有其他弹窗时保留状态栏入口。关闭或选择 **Stay in Restricted Mode**
  后，同一 workspace 不会反复自动提示；仍可点击 **Restricted Mode** 或使用
  `Ctrl-Cmd-S` 打开。
- 补齐原有信任弹窗的 `menu::Cancel` 处理，使 Escape 关闭弹窗并保留受限状态。
- **Trust and Continue** 沿用上游逻辑：只信任列出的项目，父目录选项默认不勾选；
  发送远端信任消息，并由 workspace 的现有订阅保存信任路径。
  重新连接时沿用上游按远端主机、用户与路径恢复信任的机制。

没有新增依赖，也没有设置 `trust_all_worktrees`。

## 构建与源码还原

`scripts/prepare-source.py` 在修改源码前检查 Cargo build 入口和补丁上下文。
重复应用或上游变动会报错，需检查原因后处理。CI 缓存 key 包含补丁摘要。

产物中的 `source.patch` 通过 `git diff --binary HEAD` 生成，覆盖已暂存和未暂存的
已跟踪改动，包括两个 iOS 源文件和共用的信任弹窗。源码归档仍是上游基线，需配合补丁使用。
今后新增未跟踪的上游源文件时，必须另外处理归档覆盖范围。

## 验证记录

2026-09-16 完成以下验证：

- 基于原构建的完整上游源码归档应用补丁，只修改三个 Rust 文件、Cargo.lock 和
  Cargo build phase。重复应用与上下文漂移均在改写其他输入前失败。
- 源码归档加最终补丁可还原全部文件、文件模式和符号链接；已暂存改动也包含在内。
- Python/Bash 语法、Rust 解析和补丁空白检查通过。
- [构建 #5](https://github.com/yly-25S/zed-ios/actions/runs/35033182300) 成功，
  实际构建提交为 `e80fb1f02c127cd6c9a8b9de7ff02615ada5f65a`。
  环境为 macOS 26.6.2 arm64、Xcode 26.6、iPhoneOS SDK 26.5、Rust 1.94.1。
  CI 实际解析的锁文件摘要与仓库审核值相同。
- 下载[构建产物](https://github.com/yly-25S/zed-ios/actions/runs/35033182300/artifacts/10422379269)
  后，全部 SHA-256、两种 ZIP 完整性、IPA/App 内容一致性和源码还原检查通过。
  Mach-O 为 arm64 / iOS 真机平台，最低 17.0；最终 plist 的设备类型为 `[2]`，
  Bundle ID 为 `io.github.yly25s.zed.ipad`，版本仍是 `1.0 (1)`。

构建 #4 是发现 Escape 取消路径遗漏后主动取消的过时运行，未进入 Xcode 编译。
真机交互与真实远端 LSP 验收尚未执行。

## 真机验收

构建成功仅能验证编译与打包，以下交互需要在 iPad 上验证。测试时使用与客户端
协议兼容的 remote server，并检查这次连接的日志。

1. 使用尚未信任的远程测试项目连接。预期出现 **Unrecognized Project** 弹窗，
   显示正确的项目路径及远端用户、主机，父目录选项默认关闭。
2. 选择 **Stay in Restricted Mode**，或按 Escape。应返回编辑器，状态栏仍有
   **Restricted Mode**，语言服务器仍受限；重复信任状态消息不应反复弹窗。
3. 点击状态栏按钮，检查可以重新打开；再用 `Ctrl-Cmd-S` 检查键盘入口。
   在弹窗中点击 **Trust and Continue** 或按 Enter。
4. 检查弹窗关闭、焦点返回编辑器、受限按钮消失；服务端结束信任等待，开始启动
   对应语言服务器。输入暂停后手动触发补全，再验证连续输入的自动补全。
5. 等待数据库保存后断开重连，再完全退出并重启应用。相同用户、主机与项目路径
   应恢复信任；同一主机未信任的相邻项目和另一主机的同路径项目仍应要求信任。
6. 在两个 workspace 间切换，并返回连接页。后台连接收到限制消息时不应抢焦点；
   切回受限项目后，其按钮应可以打开正确项目的弹窗。
7. 在横屏、竖屏及分屏窗口中检查路径可读、按钮可点击、弹窗可关闭和焦点恢复。

若信任后服务端已启动语言服务器，但补全仍异常，需要另查补全响应与文本同步；
本补丁不修复其他 SSH 认证方法或历史补全范围错误。
