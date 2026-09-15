# Zed for iPadOS / iOS

[![macOS preflight](https://github.com/yly-25S/zed-ios/actions/workflows/macos-preflight.yml/badge.svg)](https://github.com/yly-25S/zed-ios/actions/workflows/macos-preflight.yml)
[![Build Zed for iOS](https://github.com/yly-25S/zed-ios/actions/workflows/build-ios.yml/badge.svg)](https://github.com/yly-25S/zed-ios/actions/workflows/build-ios.yml)

使用 GitHub Actions 的 macOS runner 构建 [dcow 的 Zed for iPad PR #52921](https://github.com/zed-industries/zed/pull/52921)。
这是独立的构建仓库；每次 CI 都检出完整上游源码，固定在 PR 的提交
`3440251b30d5c5b522d03be285ab794dcb96bcd5`，再生成未签名的 iOS 应用。

仓库中的 `Cargo.lock.ios` 修复上游锁文件的一处过期引用：`dev_container` 的
`env_logger 0.11.8` 改为已在上游锁文件中的 `env_logger 0.11.10`。
未添加、删除或升级任何锁定的第三方包。

已验证：macOS 26.6.2、Xcode 26.6、Rust 1.94.1。
[完整构建 #3](https://github.com/yly-25S/zed-ios/actions/runs/34992543936) 已通过编译、链接、arm64/iOS 检查和打包；
[下载该次产物](https://github.com/yly-25S/zed-ios/actions/runs/34992543936/artifacts/10407450650)。

目标为 **arm64 iPad，iPadOS 17 或更新版本**。保留上游的 iPad 设备支持设置，尚未适配 iPhone。
该移植在 iPad 上渲染界面、处理输入，通过 SSH 使用远端 Mac/Linux 主机进行开发。

## 下载与重新构建

进入 [Build Zed for iOS](https://github.com/yly-25S/zed-ios/actions/workflows/build-ios.yml)，
选择成功的运行，在 **Artifacts** 下载 `Zed-iPadOS-unsigned-<编号>`。
产物保留 30 天；可以通过 **Run workflow** 随时重新构建。

也可以使用已登录的 `gh`：

```bash
gh workflow run build-ios.yml --repo yly-25S/zed-ios
gh run list --repo yly-25S/zed-ios --workflow build-ios.yml
# 将 RUN_ID 替换为成功运行的编号
gh run download RUN_ID --repo yly-25S/zed-ios \
  --pattern 'Zed-iPadOS-unsigned-*' --dir artifacts
cd artifacts
shasum -a 256 -c SHA256SUMS
```

| 文件 | 用途 |
| --- | --- |
| `Zed-iPadOS-unsigned.ipa` | `Payload/Zed.app` 格式的设备包，需自行签名 |
| `Zed-iPadOS.app.zip` | 保留 App bundle 结构的未签名应用 |
| `zed-source.tar.gz` | 对应 PR 提交的完整源码 |
| `source.patch` | 构建时的修改：补齐 iOS 依赖锁文件、强制 Cargo 使用 `--locked` |
| `build-info.txt` | 源码/构建脚本提交、Xcode/Rust 版本、构建配置 |
| `SHA256SUMS` | IPA、App ZIP、源码与补丁的 SHA-256 校验值 |
| `LICENSE*` | 上游许可证 |

另有 `Zed-iPadOS-build-logs-<编号>` 保存编译日志和二进制检查信息，保留 14 天。

## 安装与使用

**IPA 未签名，不能直接在普通 iPad 上安装，也不是 TestFlight/App Store 包。**
本仓库不需要 Apple 证书即可完成编译；安装前应使用自己的签名证书和匹配的 provisioning profile。

在 Mac 上使用 Xcode 从源码安装：

```bash
mkdir zed-ipad-source
cd zed-ipad-source
tar -xzf ../artifacts/zed-source.tar.gz --strip-components=1
patch -p1 < ../artifacts/source.patch
rustup target add --toolchain 1.94.1 aarch64-apple-ios aarch64-apple-ios-sim
open ios/Zed.xcodeproj
```

在 Xcode 的 **Signing & Capabilities** 中为 Zed target 选择自己的 Team，
将 Bundle Identifier 改为自己可以签名的唯一值，选择连接的 iPad 并运行。
CI 产物使用 `io.github.yly25s.zed.ipad`，命令行关闭了签名并清空了上游作者的 Team 设置。

启动后在连接界面填写远端 SSH 主机、用户名及项目路径。
该提交的 iOS SSH 实现会选择远端 `~/.zed_server/zed-remote-server-*` 中修改时间最新的服务端。
需要先从桌面版 Zed 连接该主机以部署服务端，或者把对应源码构建的服务端手动放到这个目录。
iPad 客户端本身尚未实现自动下载/上传服务端，详见
[实际的服务端查找代码](https://github.com/dcow/zed/blob/3440251b30d5c5b522d03be285ab794dcb96bcd5/crates/remote/src/transport/russh_ssh.rs#L166)。
服务端的其他设置与限制见
[固定版本的上游 iOS README](https://github.com/dcow/zed/blob/3440251b30d5c5b522d03be285ab794dcb96bcd5/ios/README.md)。
本构建固定在实验性 PR 的版本；远端协议若不兼容，需要使用对应源码的服务端。

## 构建方式

- `macOS preflight`：实际启动 `macos-26` runner，编译引用 UIKit 的 arm64 iOS Swift 文件。
- `Build Zed for iOS`：检出固定源码 → 安装源码指定的 Rust 1.94.1 → 应用仓库保存的 `Cargo.lock.ios` 和 Cargo lock 补丁 →
  通过 Xcode 的 Cargo build phase 构建 `zed_ios` 静态库 → 链接 Swift/UIKit App → 校验 arm64 二进制并打包。
- 使用 Debug 配置，禁用 Rust debug symbols 和 incremental，保留上游嵌入字体、主题等资源的设置。
  Cargo 并行度为 3，以适配标准 macOS runner。
- 不预先单独运行一次 Cargo build，避免 Xcode build phase 再编译一次。
- 固定第三方 Actions 的 commit，工作流只使用 `contents: read` 权限。
- 完整构建只在手动触发时运行。默认严格使用 `Cargo.lock.ios`；维护者可以勾选 `refresh_lock`
  重新解析缺失依赖，运行会单独上传实际锁文件，审核后可将它保存回仓库。

如需在本地 Mac 复现 CI，在本仓库内准备 `source/`：

```bash
git init source
git -C source remote add origin https://github.com/dcow/zed.git
git -C source fetch --depth 1 origin 3440251b30d5c5b522d03be285ab794dcb96bcd5
git -C source checkout --detach FETCH_HEAD
python3 scripts/prepare-source.py source
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=3
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=off
export CARGO_PROFILE_DEV_BUILD_OVERRIDE_DEBUG=false
export IPHONEOS_DEPLOYMENT_TARGET=17.0
scripts/build-ios.sh
```

编译成功表示设备目标通过 Rust/Swift 编译、链接和打包检查；本仓库的 CI 不进行真机运行验证。
上游仍有中文输入法组合输入、部分扩展、账号登录和 SSH 主机密钥校验等未完成事项，见
[该版本的 checklist](https://github.com/dcow/zed/blob/3440251b30d5c5b522d03be285ab794dcb96bcd5/ios/checklist.md)。

## 来源

Zed 和 iPad 移植源码归各自原作者所有；源码及产物附带上游许可证。
本仓库提供构建自动化，没有将该 PR 表述为 Zed 官方支持的 iOS 版本。
