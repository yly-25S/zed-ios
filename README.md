# Zed for iOS / iPadOS — reproducible CI builds

Build automation for [Zed PR #52921](https://github.com/zed-industries/zed/pull/52921)
by [dcow](https://github.com/dcow). Source is pinned to PR commit
`3440251b30d5c5b522d03be285ab794dcb96bcd5`.

The upstream port targets iPad and connects to a remote development host over SSH.
Device builds are unsigned and require your own Apple signing identity before installation.

First, the `macOS preflight` workflow verifies a GitHub-hosted macOS runner and
compiles a UIKit source file for arm64 iOS. The iOS build workflow follows once
that check succeeds.
