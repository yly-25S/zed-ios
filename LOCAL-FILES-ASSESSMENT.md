# iOS 本地文件编辑可行性评估

日期：2026-09-16。基于已构建 PR 提交 `3440251b30d5c5b522d03be285ab794dcb96bcd5` 的源码检查，以及 Apple 和 Theos 官方资料。本文是设计评估，没有实现补丁、运行新的 iOS 构建或进行真机验证。

## 结论与范围

**未越狱即可实现本地代码编辑，不需要 remote server。** 可访问应用容器内的文件，以及用户通过系统文件选择器授权的文件或目录。当前 PR 主动采用远程项目入口，并非 Zed 编辑器核心不能处理本地文件。

应区分三个目标：

1. 本地编辑器：打开、修改、保存文本，语法高亮、文件树、搜索、撤销和会话恢复。
2. Files 集成：原位修改用户授权目录，处理 iCloud、第三方 File Provider、外置磁盘的授权、同步和冲突。
3. 本地开发环境：Git、LSP、格式化器、终端、编译器和扩展执行。这一层涉及进程与运行时，复杂度远高于读写文件。

普通未越狱环境可以完成前两层；第三层需要逐项改为进程内实现或远程执行。越狱为第三层提供更多实现空间，但不自动恢复桌面后端兼容性。

## 当前源码可以复用什么，缺什么

所有源码路径均相对于上述固定提交的完整 Zed 源码；本机构建仓库的 `source/` 为稀疏检出，完整源码另在 `../zed-remote-server-build/` 和成品源码归档中。

| 模块 | 已确认状态 | 改动含义 |
| --- | --- | --- |
| `crates/zed_ios/src/lib.rs:372` | 已初始化 `RealFs`，同时初始化语言注册表、编辑器、workspace 和项目面板 | 本地读写和编辑内核有现成基础，不必重写编辑器 |
| `crates/zed_ios/src/lib.rs:537` | 每个 workspace 的打开路径回调被设为空操作 | 接入实际文件选择和本地打开流程 |
| `crates/zed_ios/src/connection_landing.rs:1449` | 连接流程构造 `Project::remote` | 保留 SSH 入口，同时新增不依赖连接的本地项目入口 |
| `crates/workspace/src/workspace.rs:1789`、`9411` | 存在 `Workspace::new_local` 和 `workspace::open_paths` | 优先复用，并适配当前 iOS 的单窗口/replace_root 流程 |
| `crates/project/src/project.rs:1144` | `Project::local` 未被 iOS 条件编译删除，但会初始化 LSP、Git、任务、DAP 等服务 | 不能只改一行构造函数；需要关闭或替换不支持的本地执行能力 |
| `crates/gpui_ios/src/platform.rs:402`、`416` | `prompt_for_paths` 和 `prompt_for_new_path` 返回未实现/不支持错误 | 补 UIKit 选择器、保存/导出入口及 GPUI 异步结果桥接 |
| `crates/fs/src/fs.rs:895` | `save` 直接创建文件并写入 Rope；另有独立 `atomic_write` 方法 | 普通保存不能被误认为已经使用原子替换；需要审计完整保存路径 |
| `crates/fs/src/fs_watcher_ios.rs` | 已有 GCD vnode watcher | 需要完善和测试，不能写成“监听完全不存在” |
| `crates/worktree/src/worktree.rs:4744` | 扫描后为目录增加 watcher | 目录变化有基础，但子文件原地内容修改、资源回收等仍需验证 |
| `crates/language/src/language.rs:137` | iOS 使用原生 Tree-sitter parser，跳过 Wasm store | 已静态嵌入的语法高亮可复用，不要求 LSP/JIT |
| `crates/git/src/repository.rs:951` | 虽然依赖 libgit2，构造仓库仍要求存在 Git 二进制 | 不能假定 libgit2 已链接就能直接开启完整本地 Git |
| `ios/Zed/SceneDelegate.swift`、`AppDelegate.swift` | 当前没有文件 URL 打开入口；失活路径主要记录 SSH 会话 | 补文件打开、授权恢复、脏缓冲区持久化与前后台处理 |

## 未越狱：可访问范围

这里的“未越狱”指正常签名和普通应用权限，包括通常的侧载安装；签名方式本身不应被当成任意文件权限或执行权限。依赖漏洞获得特殊 entitlement 的安装方案需要独立评估，不能归入普通 IPA 的能力基线。

- 应用自己的容器：可建立本地项目目录，不需要越狱。
- 用户通过系统选择器授权的目录：可递归访问授权范围；需要维护 security-scoped URL 的生命周期及持久化书签。[Apple 目录授权说明](https://developer.apple.com/videos/play/wwdc2019/719/)
- Files 中的 iCloud、第三方提供方或外置存储：取决于提供方支持的操作、用户授权和文件可用状态；“Files 中看得到”不代表文件已离线下载或一定可写。[Apple 文件管理说明](https://developer.apple.com/videos/play/wwdc2019/719/)
- 其他应用未共享的私有目录和系统文件：普通应用不能任意遍历或修改。[Apple 运行时安全说明](https://support.apple.com/guide/security/security-of-runtime-process-sec15bfe098e/web)

把项目复制进应用容器后编辑是最容易的第一阶段，但它是副本工作流，不能向用户描述为原位修改来源文件。原位编辑使用 `UIDocumentPickerViewController(forOpeningContentTypes:asCopy:)` 的打开语义，文件与文件夹选择需适配各自类型和当前 GPUI 接口。

## 未越狱：实施分解

### 1. 打开入口与本地项目：中等

- 首页增加本地文件、文件夹和已有本地项目入口。
- 替换空的打开回调，通过 UIKit/GPUI 桥接得到选择结果，交给本地 workspace。
- 适配当前 window/root 替换与返回首页逻辑；已有连接列表和会话恢复是按 SSH host 组织的，不能直接承载本地路径。
- 首个原型先用容器内小项目，确认读取、编辑、保存、重新打开和内置语法高亮形成闭环。

### 2. 能力隔离：中高，必须早做

`Project::local` 会连接许多本地服务。`NodeRuntime::unavailable()` 只解决 Node 的一部分入口，不会自动禁止原生 LSP、Git 命令、外部 formatter、任务、DAP、MCP/ACP 或 shell 环境探测。

建议增加明确的项目能力策略，按“本地沙盒编辑 / SSH 远程 / 越狱本机后端”选择。它是拟议改动，不是现有 API。不要仅用全局 `cfg(ios)` 关掉全部功能，否则会误伤仍然可用的远程工作区。

第一阶段保留文件、buffer、搜索、内置语法解析，禁用不具备实现的自动启动、下载和命令执行，并给相关操作准确的不可用状态。打开仓库中的源码文件不应强制要求 Git 功能也可用。

### 3. 授权与保存：高，是原位编辑的主要难点

- 选择器返回的 URL 不能只转成路径字符串后丢弃。需要一个随项目存活的授权对象，平衡 start/stop access，保存并恢复书签，处理过期、撤权、移动和重装后的失效。
- GPUI 当前回调只返回 `PathBuf`；可在 iOS 侧维护项目授权注册表，而不必立即修改所有桌面接口。生命周期不能止于选择器回调结束。
- 外部文件的读、写、枚举、移动和删除需要协调。建议先评估在 `Fs` 抽象后增加 iOS 协调层，私有文件继续复用 `RealFs`，外部文档通过 Foundation 协调。[Apple NSFilePresenter](https://developer.apple.com/documentation/foundation/nsfilepresenter?changes=_2)
- 覆盖 `save`、`write`、`atomic_write`、`open_sync` 等实际入口；当前搜索会取得流式 reader，协调只包住“打开 fd”未必能保护整个读取过程，需要考虑快照读取或与 reader 绑定的访问期限。
- 单文件授权不应被假定为有权在父目录创建临时文件。原子替换、另存为和临时目录必须尊重授权与卷边界，并检查失败时是否保留原文件。
- 使用 `NSFileCoordinator` 并不等于自动解决所有内容冲突；还需要检测外部版本变化，保留本地未保存编辑，提供重载、保留副本或冲突处理。

### 4. Files 集成与文件监听：中高

- 为应用自己的用户文档选择性配置 `UIFileSharingEnabled` 和 `LSSupportsOpeningDocumentsInPlace`；补文件类型声明和 scene 文件 URL 入口。上述两个 key 能让 Files 访问 Documents，但不能替代打开、编辑和保存代码。[Apple plist 文档](https://developer.apple.com/library/archive/documentation/General/Reference/InfoPlistKeyReference/Articles/LaunchServicesKeys.html)
- 缓存、数据库、授权书签等内部数据继续放在适当的 Library 子目录，避免把整个内部状态当作文档暴露。
- 当前 vnode watcher 监听单个路径；worktree 会对扫描过的目录加监听，但这不证明子文件的每次原地写入都能触发准确通知。需覆盖创建、删除、改名、原子替换、内容原地修改和重复开关项目。
- 代码中的 dispatch context 持有 `Arc<WatchState>`，而 state 又持有 dispatch source；`remove` 当前只从 map 移除引用。存在需要验证的回收/取消循环风险，不能在未测试前宣称已经泄漏或已经正确回收。
- 文件提供方通知补充使用 `NSFilePresenter`；它只能保证协调访问产生的通知，不涵盖所有直接 POSIX 写入，仍需 vnode 或恢复前台后的补扫。[Apple NSFilePresenter](https://developer.apple.com/documentation/foundation/nsfilepresenter?changes=_2)

### 5. 生命周期、性能与输入：中高

- 经常保存本地恢复快照，补 scene 进入后台、恢复前台和文件授权恢复。不能只在“即将退出”保存，进程可能不经过正常退出。
- 若自管 `NSFilePresenter`，按照 iOS 生命周期移除和恢复 presenter，避免应用挂起时阻塞其他进程的协调访问；`UIDocument` 可代管这部分，但需桥接到现有 Zed buffer。[Apple NSFilePresenter](https://developer.apple.com/documentation/foundation/nsfilepresenter?changes=_2)
- 对项目扫描、全文搜索、文件物化和 watcher 数量设置并发与资源上限。远程模式曾由服务器承担的开销将转移到 iPad；不能假定所有目录已下载或可立即访问。
- 已有 CJK/IME 和 UTF-16 输入相关待办会影响本地编辑质量，但它们是编辑输入层问题，不是文件权限问题，应单列验证与工作量。

### 6. 本地开发工具：高至很高，另立范围

| 能力 | 普通未越狱设备的实现路线 |
| --- | --- |
| 高亮、缩进、文本查找、文件树 | 复用进程内 Rust/Tree-sitter 能力，重点补集成与资源管理 |
| Git | 可以研究完整的进程内 libgit2 后端；现实现混用 Git CLI，需重构、凭据与传输适配 |
| LSP、格式化 | 选择可嵌入的特定语言服务/库，或做独立远程桥接；不是启动现有二进制即可 |
| 终端、任务、编译器 | 普通应用不能按桌面模式任意创建工具子进程；嵌入解释器/模拟环境是不同且更大的方案 |
| Wasm 扩展 | 纯数据配置和原生内置语法可用；现有 Wasmtime/子进程扩展不能直接照搬，可另评估解释执行等方案 |

将本地项目连接到远端 LSP 也并非现有 `Project::remote` 自动提供：需要解决文件 URI 映射、未保存 buffer 同步、未打开文件及依赖可见性、重命名和构建产物等一致性问题。这是混合项目后端项目，不应塞入本地编辑 MVP。

平台进程、签名和 JIT 限制参考 [Apple 运行时安全](https://support.apple.com/guide/security/security-of-runtime-process-sec15bfe098e/web)；此处未展开 App Store 审核结论。

## 越狱：分别讨论两条路线

越狱能力取决于具体设备、系统版本、越狱方案、签名、沙盒状态和进程权限；普通安装的 App 不会因为设备越狱就必然拥有所有权限。rootless 主要描述安装布局，不表示没有提权能力，也不表示系统分区任意可写。Theos 使用 `/var/jb` 及可重定位 root 路径、`@rpath` 等机制。[Theos rootless 文档](https://theos.dev/docs/rootless)

### A. 扩展本地工作区的直接文件访问：中等增量

复用未越狱本地编辑架构，增加经验证的文件访问权限、应用内路径浏览和明确的访问失败处理。访问范围可以扩展到该进程确实可读写的目录；UID、沙盒、数据保护、只读卷仍是不同限制。

这条路线对“编辑本地源码、配置文件”最直接。文件树、保存、监听、会话恢复仍需实现；Files/iCloud 场景仍要协调，不能因为越狱就删除这层逻辑。若支持 root 权限写入，应由范围明确的 helper 承担必要操作，而非让整个联网编辑器和所有项目工具默认以 root 运行。

### B. iPad 上运行同源 remote server，客户端连回本机：高

概念结构：`Zed iPad UI → loopback SSH/本地传输 → iOS remote_server → 本地文件/Git/LSP/PTY`。

优点是复用现有远程项目、文件树、保存和协议，将“远端”变为同一台 iPad。若同源服务端确实能运行，可减少 UI 层的新本地项目代码；它也能给开发工具提供不同于 UI 进程的运行环境。

但当前还存在明确的移植障碍：

1. **二进制目标不同。** Linux arm64 与 macOS arm64 成品都不能因 CPU 架构相同就当作 iOS CLI 运行；需构建对应 iOS 目标，验证依赖、动态库和签名。
2. **headless 平台未完成。** `gpui_platform::current_platform` 的 iOS 分支忽略 `headless` 参数，创建带 UIKit/屏幕初始化的 `IosPlatform`；其 `run()` 依赖 UIKit 驱动循环并立即返回。独立 CLI 不能直接依赖这个 UI 生命周期，需实现可持续工作的无窗口执行器/运行循环。
3. **服务端依赖比客户端更多。** remote_server 引入 extension_host/Wasmtime、NodeRuntime、crash handler、进程管理等。客户端 IPA 编译成功，不证明这个依赖集合可以为 iOS 编译和运行。
4. **平台探测会误判。** `remote/src/transport.rs:29` 把 `uname` 的 `Darwin` 归为 `RemoteOs::MacOs`，且枚举没有 iOS。即使手工放置服务端绕过下载，也需审查后续工具下载、路径与平台假设，不能冒充 macOS 来掩盖差异。
5. **工具链需要分别移植。** Git、shell、PTY、Node 和具体语言服务器要有该越狱环境可用的包；不是恢复 `Command` 就能运行任意桌面工具。
6. **运行权限与安装布局。** 需要匹配 rootless/rootful、依赖搜索路径、启动方式及服务运行用户；loopback SSH 的 host key/认证与本地传输隔离也需验证。
7. **资源与生命周期。** UI 与服务端同在一台设备，内存、电量和散热成本会叠加；越狱或 daemon 不能被当作无条件免疫系统资源回收。

先做一个最小 headless/protocol 探针，验证持续事件循环、Ping/Ack 和读写目录，再接 Git/LSP。只有这一步成功，才能缩小整体工期的不确定性。

## 难度与工作量估算

以下为一名熟悉 Rust/GPUI 与 UIKit 的工程师、可持续使用 Mac CI 和真实 iPad 的粗略人日评估，不是实测工期。不含解决全部现有输入缺陷、任意语言工具链或所有文件提供方兼容性。

| 交付范围 | 难度 | 估计 |
| --- | --- | --- |
| 容器内本地项目 + 导入/导出，基本编辑保存闭环 | 中 | 5–10 人日 |
| 可日常使用的本地项目，补能力隔离、恢复与基本目录监听 | 中高 | 累计约 10–20 人日 |
| Files 授权目录原位编辑，含书签、协调保存、冲突及常见提供方测试 | 高 | 累计约 20–40 人日 |
| 在上述基础上支持一种已确认越狱环境的扩大路径访问 | 中等增量 | 约增加 3–10 人日，权限模型未知时不可承诺 |
| 越狱本机 remote_server 路线 | 高，风险集中在后端移植 | 先用 2–5 人日探针评估；完整方案按数周至数月估算 |
| 未越狱完整桌面式本地工具链与扩展生态 | 很高 | 不是固定几周的适配；需按语言/运行时单独立项 |

## 建议推进顺序与验收

1. 用应用容器内项目验证 `Workspace::new_local`、能力隔离、读取/编辑/保存和静态高亮，保持 SSH 模式可用。
2. 接系统文件选择器。先明确“导入副本”与“原位编辑”两个工作流，再加入目录授权、书签和协调文件系统。
3. 验证外部修改与脏 buffer 冲突、原子替换、只读目录、授权撤销、应用被终止后恢复、离线云文件和外置磁盘断开；失败保存不能显示为已保存。
4. 对目录/文件规模分档测试，检查 scanner、文件描述符、内存及前后台恢复；验收已有中文输入问题对实际编辑的影响。
5. 越狱只编辑文件时沿用上述核心并扩展授权；需要本机终端/LSP 时再验证 headless 服务端路线。

目前不需要恢复此前暂停的 Linux remote server 安装来完成这项分析，也不需要先修 SSH 密码认证才能实现未越狱的本地编辑器。
