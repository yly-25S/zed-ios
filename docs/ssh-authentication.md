# iPad SSH keyboard-interactive 认证

`patches/ios-keyboard-interactive.patch` 应用于固定 PR 源码，使用原有的 russh **0.58.0**，不升级应用依赖。
它解决服务器只允许 `publickey,keyboard-interactive` 时，客户端反复调用 `password` 并误报密码错误的问题。

## 修复行为

- 先查询服务器允许的方法，再尝试应用内已有的本地密钥、SSH password 和 keyboard-interactive。
- password 方法可以使用连接配置中保存的密码；被拒绝后可以重新输入。
- keyboard-interactive 显示服务器名称、说明和全部提示，按 `echo` 标记决定是否隐藏输入；支持多轮、多个和零个提示。
- keyboard-interactive 的任何字段都不会自动填入保存的密码，也不会保存回答。用户名、OTP 和 PAM 密码均由用户明确输入。
- 空协议提示返回空字符串；零字段但带说明的请求显示说明并等待 Continue。每轮回答保持服务器要求的顺序。
- 部分成功继续完成服务器要求的下一因素。每个认证阶段中，password 和 keyboard-interactive 各最多尝试两次。
- Cancel、Escape 或 Ctrl-C 取消当前认证，并停止本次自动重连；Enter 提交，Tab / Shift-Tab 切换输入框与按钮。
- 连接页和 workspace 重连共用弹窗；并发认证排队显示主机信息。已有其他 workspace 弹窗时提示先关闭弹窗再重连。
- 无可用方法、凭据被拒绝和取消使用不同错误信息；日志不记录回答或包含回答的发送错误。

本地密钥支持范围仍是应用 HOME 下 `.ssh/id_ed25519`、`id_ecdsa`、`id_rsa` 的无口令加载。
本补丁不增加 OpenSSH agent、keyboard-interactive 答案自动保存或 remote server 部署功能。

## 自动验证

协议测试直接编译应用实际使用的 `russh_auth.rs`，通过内存双向流连接真实的 russh 客户端与模拟服务器，
不访问现有 SSH 服务器，也不修改 sshd/PAM 配置。

```bash
# source/ 必须是原始固定提交的完整检出；已经准备过的源码不要重复执行。
python3 scripts/prepare-source.py source
ZED_IOS_SOURCE_DIR="$PWD/source" \
CARGO_TARGET_DIR="$PWD/.work/ssh-auth-target" \
cargo +1.94.1 test --manifest-path tests/ssh-auth/Cargo.toml --locked --jobs 3
```

15 项测试覆盖 password-only、保存密码被拒绝后的重试、PAM、密码到 keyboard-interactive 的回退、
多轮和多字段、echo、空提示与零字段说明、错误凭据、取消、不可用方法、
服务器要求的连续因素、部分成功后的新阶段、回答数量错误及无需认证的连接。
测试锁文件只使用 `Cargo.lock.ios` 中已有的第三方包版本。
russh 0.58 测试服务器会清除 password/public-key 拒绝包中的 partial-success 标记；
因此标记本身及阶段重置使用 keyboard-interactive 的真实协议包验证。

源码归档与补丁验证（输入应为构建产物中原始的 `zed-source.tar.gz`）：

```bash
python3 tests/verify-source.py artifacts/zed-source.tar.gz
```

验证补丁应用范围、锁文件摘要、重复应用/上下文漂移的拒绝行为，及所有文件、权限、符号链接的完整还原。
`prepare-source.py` 使用 `git apply --intent-to-add`，使新增源文件也进入打包的 `git diff --binary HEAD`；
已暂存的修改同样包含在源码补丁中。

## 已完成的构建验证

2026-09-18（Asia/Chongqing），[完整构建 #10](https://github.com/yly-25S/zed-ios/actions/runs/35247212631)
成功，构建提交为 `9a7ba6b409249a10fdffbff61e0bbac4965a8f4c`。
[下载未签名应用与对应源码](https://github.com/yly-25S/zed-ios/actions/runs/35247212631/artifacts/10508209026)。

- 本地 15 项协议测试通过；实际 iOS 目标通过 Rust/Swift 编译、链接和打包。
- 实际工具链：macOS 26.6.2、Xcode 26.6、iPhoneOS SDK 26.5、Rust 1.94.1。
- 已下载到 `artifacts/keyboard-interactive-build-10/`，核对 GitHub 整包摘要、全部 `SHA256SUMS`、
  IPA/App ZIP 完整性及应用文件的一致性。
- 成品为 arm64、iOS 真机平台、最低 17.0；最终 plist 为 iPad `[2]`、
  `io.github.yly25s.zed.ipad`、版本 `1.0 (1)`。
- 源码归档加 `source.patch` 逐文件还原为审核后的完整源码，权限、符号链接及两个新增认证源文件均一致；
  `Cargo.lock.ios` 摘要保持不变。
- 可选缓存恢复步骤命中 #9 的已审核依赖缓存，并将本次结果保存到当前补丁的独立缓存键。

IPA SHA-256：`80754ce4e0b6615063f19cf6a3cc342a7902eb4ce081ca2788ac0fc42f94500c`。
keyboard-interactive 的 iPad 真机认证与提示交互仍待下面的验收。

## iPad 验收

编译和协议测试不替代设备上的提示交互验证。新包应逐项检查：

1. 原有 password-only 主机仍能连接，错误密码后可重试或取消。
2. 仅允许 keyboard-interactive/PAM 的主机显示服务器密码提示，输入后连接成功。
3. 需要 OTP 或多轮认证时，不出现保存密码的自动填充；每轮提示、隐藏/显示与回答顺序正确。
4. Cancel、Escape、关闭连接及重连不会留下旧弹窗或继续发送回答。
5. workspace 中断线重连可显示提示；多台主机自动连接时弹窗不会互相覆盖。
6. iPad 横竖屏、软键盘及外接键盘下，所有输入框和 Continue/Cancel 均可操作。

设备验证前，不把 keyboard-interactive 的真机连接写成已完成。
