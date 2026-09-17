# Zed iPadOS 移植与构建维护指南

本文件整理 2026-09-16 本次移植得到的经验，供后续代理维护这个构建仓库时使用。历史环境和成功记录是复现基线，不代表未来 runner 或上游版本仍然相同。

## 仓库职责与当前基线

- 本仓库是 `yly-25S/zed-ios` 的构建自动化仓库；完整 Zed 源码在构建时另行检出，不要把此仓库当作 Zed 源码 fork。
- 来源为 [Zed PR #52921](https://github.com/zed-industries/zed/pull/52921)，固定 `dcow/zed` 提交 `3440251b30d5c5b522d03be285ab794dcb96bcd5`。这是实验性 iPad 远程客户端，不是官方支持的 iOS 发行版。
- 已验证目标：arm64 真机、iPadOS 17+；没有完成 iPhone 适配。iPad 本地渲染界面和处理输入，开发能力依赖 SSH 远端。
- [完整构建 #3](https://github.com/yly-25S/zed-ios/actions/runs/34992543936) 成功，构建自动化提交为 `73d9616b83e6f663e3728a5b63e5b25462cc3bdc`。当时环境为 macOS 26.6.2 arm64、Xcode 26.6、iPhoneOS SDK 26.5、Rust 1.94.1；完成约需 20 分钟。
- [信任修复构建 #5](https://github.com/yly-25S/zed-ios/actions/runs/35033182300) 成功，构建提交为 `e80fb1f02c127cd6c9a8b9de7ff02615ada5f65a`，使用同样版本的工具链。已核对下载产物的摘要、ZIP、最终 plist、arm64/iOS 17 平台和源码还原；2026-09-17 用户真机验证后反馈信任修复功能正常，详见 `docs/workspace-trust.md`。
- 成功范围是编译、链接、二进制检查和未签名打包；不能据此声称真机交互、SSH 认证、远端协议或安装签名均已验证。

## 文件分工

| 路径 | 用途 |
| --- | --- |
| `.github/workflows/macos-preflight.yml` | 用小型 UIKit Swift 编译验证 macOS runner 和 iOS 工具链 |
| `.github/workflows/build-ios.yml` | 手动触发完整构建、缓存与产物上传 |
| `scripts/prepare-source.py` | 应用 iOS 信任补丁、审核过的锁文件，并在上游 Cargo build phase 加入 `--locked` |
| `patches/ios-workspace-trust.patch` | iOS workspace 信任初始化、状态栏入口和首次提示；验收见 `docs/workspace-trust.md` |
| `scripts/build-ios.sh` | Xcode 构建、检查、打包及对应源码归档 |
| `Cargo.lock.ios` | 当前源码对应的审核后依赖锁文件 |
| `source/` | 忽略的上游源码目录；本次本地目录是稀疏检出，CI 检出完整源码 |
| `artifacts/` | 忽略的本地下载或构建产物 |
| `.work/` | 忽略的临时源码、日志和诊断工具 |
| `NEXT-STEPS.md` | 若存在，为忽略的本地交接记录；包含暂停状态和私有环境细节，不得上传 |
| `LOCAL-FILES-ASSESSMENT.md` | 本地文件编辑的源码评估、未越狱/越狱路线与难度估算；是设计方案，尚未实现 |

## 先确认工具链，再运行完整构建

1. 新账户或新环境先检查 `gh` 登录和目标仓库权限，再实际运行 macOS 预检；仅凭存在 `gh` 或能创建仓库不能证明 macOS Actions 可用。
2. 预检要真正编译 `import UIKit` 的 Swift 文件，目标为 `arm64-apple-ios17.0`，并检查生成对象的架构和平台。仅查询 Xcode 版本不够。
3. 改动二进制检查命令时，先在预检的小对象上验证。这次 Rust/Swift 编译曾成功，但耗时构建被后续错误的 lipo 参数顺序判为失败。
4. 已验证的调用形式为 `xcrun lipo "$binary" -verify_arch arm64`，输入文件放在 `-verify_arch` 前；另用 `xcrun vtool -show-build "$binary"` 检查平台和最低系统版本。
5. `macos-26` 是会更新的 runner 标签。重新构建仍应记录实际 macOS、Xcode、SDK、Rust 版本，不要把历史成功环境写成当前保证。

## 固定源码与依赖，避免无意升级

- 固定源码 SHA、第三方 Actions SHA 和源码要求的 Rust 工具链；变更任一项时明确记录原因和验证范围。
- 原始 PR 的锁文件存在一处工作区引用不一致：`dev_container` 引用 `env_logger 0.11.8`，而对应已锁定包是 `0.11.10`。本次只修复该引用，没有新增、删除或升级第三方包。不要为解决这个问题直接整体 `cargo update`。
- 当前 `Cargo.lock.ios` 的 SHA-256 为 `e6405d8ef6142954e44955e49c4947587017ce5cd65778853fc3180816f0861a`。它对应当前固定源码，换源码后需要重新审核，不能机械覆盖。
- 默认先执行 `cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-ios`，实际 Cargo 构建也必须保持 `--locked`。
- `refresh_lock` 是显式维护入口，默认关闭。使用后检查上传的依赖锁文件，对比工作区引用及全部第三方包版本，审核后再保存回仓库。
- `prepare-source.py` 要求上游脚本中恰好存在一个未修改的 `cargo build` 入口；它不是幂等脚本。重复执行报错时，先检查是否已经应用补丁或上游结构改变，不要放宽匹配或重置有价值的本地改动。

## Xcode、资源占用与缓存

- 让 Xcode 工程的 Cargo build phase 构建 `zed_ios` 静态库，再链接 Swift/UIKit App。不要先单独 Cargo build 一遍造成重复编译。
- 保留上游构建脚本中的目标选择、Rust flags、资源嵌入与平台配置；新增补丁应小而明确，避免用通用 iOS 模板覆盖上游工程。
- 本次可靠配置是 Debug、`CARGO_BUILD_JOBS=3`、`CARGO_INCREMENTAL=0`、关闭 Rust dev debug symbols 和 build override debug。不要无依据提高标准 runner 上的并行度。
- 完整工作流保持手动触发，当前超时 150 分钟，权限为 `contents: read`，checkout 不保留凭据。同一分支的新运行不会自动取消已有运行，避免反复触发占用 runner。
- Rust 缓存必须覆盖源码 SHA、工具链、目标、相关编译参数和实际锁文件。当前流程先应用 `Cargo.lock.ios`，再恢复缓存。
- 缓存命中不代表已有编译产物：本次失败运行通过 `cache-on-failure` 保存过仅有依赖下载的缓存，而同一个精确 key 的缓存不能被后续运行补写。需检查缓存恢复/保存日志，必要时有依据地调整 key；不要无限重跑期待它自行完整。
- `refresh_lock` 在缓存恢复后可能改变锁文件。此类运行的缓存不能作为新锁文件已有完整编译缓存的依据，后续应先固定审核后的锁文件。
- 如果 Xcode 已成功、只是检查或打包失败，先检查工作流保留的 recovery app 和诊断日志，区分失败阶段；不要一律归咎于 Rust 编译。

## 打包、签名与源码对应关系

- CI 使用 `generic/platform=iOS`、`iphoneos`、`ARCHS=arm64`。模拟器通过不代表真机目标通过。
- CI 关闭签名并清空原作者的 Development Team；当前 Bundle ID 为 `io.github.yly25s.zed.ipad`。产物是未签名 IPA，不能声称可直接安装，也不是 TestFlight/App Store 包。
- 安装需要用户自己的证书、匹配的 provisioning profile 和可签名 Bundle ID；构建未签名包本身不需要这些凭据。
- 不要只看命令行 build setting 推断成品信息。本次虽然传入 `CURRENT_PROJECT_VERSION`，实际 Info.plist 仍是上游写定的版本 `1.0 (1)`。若修改版本号，必须检查最终 plist。
- IPA 结构应为 `Payload/Zed.app`；使用新的临时打包目录，避免旧 Payload 文件混入。保留 App bundle 结构，并检查 ZIP 完整性。
- 发布包同时保留完整对应源码、构建修改补丁、许可证、构建元数据和校验和。本次通过 `git archive HEAD` 加 `git diff --binary` 生成源码与补丁。
- 当前补丁命令 `git diff --binary HEAD` 包含已暂存和未暂存的已跟踪改动，不包含未跟踪文件。以后新增客户端源文件或调整补丁流程时，必须确认所有实际构建输入都能从归档与补丁恢复；必要时修正归档机制。
- 核对归档还原后应用补丁的结果，尤其是锁文件和上游构建脚本。不要只附一个会变化的分支链接充当对应源码。
- 当前应用产物保留 30 天，诊断日志保留 14 天；Actions 链接不是永久存档。重要成品下载到 `artifacts/` 并验证 `SHA256SUMS`。
- 大文件下载先用 `gh run download`。代理明显拖慢下载时可诊断官方重定向链路，必要时对支持 Range 的最终官方存储端点分块重试；必须验证长度、最终摘要和 ZIP，不能拼接未校验的块。不要把 GitHub token 或临时签名下载 URL 写入日志、仓库或记录文件。

## 运行时兼容性：SSH 与 remote server

- 以固定提交的实际代码为准；上游 README 中的计划描述不等于功能已经实现。
- 当前 iOS 实现不会自动部署 remote server，而是选择远端 `~/.zed_server/zed-remote-server-*` 中修改时间最新的程序，并通过 SSH 启动 `proxy --identifier ...`。不需要新增监听端口或设置常驻系统服务。
- 优先使用同一 PR SHA 的 remote server。这个移植涉及远端协议改动，不能假定任意官方最新版或已有桌面版部署的服务端一定兼容；多个版本共存时还需检查实际选中的文件。
- 当前 `crates/remote/src/transport/russh_ssh.rs` 只尝试有限的本地密钥路径和 `authenticate_password`，没有 keyboard-interactive 支持；本次锁定的 russh 是 **0.58.0**，查 API 应使用此版本。
- SSH `password` 与 `keyboard-interactive` 是不同方法，即使 PAM 最后询问的是同一个账户密码。服务器只提供 `publickey,keyboard-interactive` 时，现客户端仍会误报 `authentication failed: incorrect password`。
- 排查时对照客户端源码、SSH 握手实际提供的方法、服务端 Match 配置和 PAM 配置。不要仅凭应用报错判断密码错误，也不要先修改服务器认证策略。
- 远程文件可编辑、有语法高亮不代表 LSP 已启动。iOS 入口原先遗漏桌面入口的 `trusted_worktrees::init`，服务端可能一直等待项目信任。仓库补丁已补齐初始化和状态栏入口；初始化须在创建项目/workspace 前完成。编译通过仍需按 `docs/workspace-trust.md` 检查真机 Restrict/Trust 消息、信任恢复与 LSP 启动。
- 实现 keyboard-interactive 时需处理多轮、多个或零提示、取消及部分成功；不能把保存的密码自动用于任意 OTP/用户名提示。错误信息应区分方法不支持与凭据被拒绝。
- OpenSSH 配置缩进不会终止 Match；重复配置需考虑首个生效值。`AuthenticationMethods` 中空格分隔备选方法序列，逗号连接同一序列所需的方法。仅在末尾加 `PasswordAuthentication yes` 不一定能启用密码认证。
- 同源 Linux remote server 已有本地构建准备，但用户在完成前停止了子代理。不要把“构建启动”写成“安装完成”；具体恢复入口和检查结果见本地 `NEXT-STEPS.md`。

## 验证与后续工作边界

- 工作流或打包修改：先做适当的脚本/预检检查，再按需要运行完整 macOS 构建；检查日志、arm64 真机平台、最终 plist、IPA 结构、校验和及源码还原。
- SSH 修复：覆盖 password-only、keyboard-interactive/PAM、错误凭据和取消路径，并验证提示交互；完成新客户端构建后仍需实际 iPad 连接验证。
- remote server 安装：编译成功后验证真实 proxy 启动及协议请求，再安装到客户端查找路径；文件存在或 `--version` 能运行不足以证明协议兼容。
- 文档整理不需要重新编译或触发 CI。2026-09-16 用户要求把未完成问题留待下次，本次暂停客户端修复和 remote server 安装；仅在后续用户要求继续时恢复相应工作。
- 服务器地址、账户配置、认证日志和安装中断细节留在被忽略的本地交接文件。可提交的维护指南只记录通用经验与公开构建信息。
