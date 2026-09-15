#!/bin/bash
set -euo pipefail

BUILD_ROOT=$(cd "$(dirname "$0")/.." && pwd)
SOURCE_DIR="$BUILD_ROOT/source"
ARTIFACT_DIR="$BUILD_ROOT/artifacts"
DERIVED_DATA="$BUILD_ROOT/.work/DerivedData"
mkdir -p "$ARTIFACT_DIR" "$BUILD_ROOT/.work"

{
    echo "Source repository: https://github.com/dcow/zed"
    echo "Source PR: https://github.com/zed-industries/zed/pull/52921"
    echo "Source commit: $(git -C "$SOURCE_DIR" rev-parse HEAD)"
    echo "Build automation commit: $(git -C "$BUILD_ROOT" rev-parse HEAD)"
    echo "Run: https://github.com/${GITHUB_REPOSITORY:-yly-25S/zed-ios}/actions/runs/${GITHUB_RUN_ID:-local}"
    echo "Configuration: Debug, arm64, iPadOS 17+, unsigned"
    echo "Cargo debug info: ${CARGO_PROFILE_DEV_DEBUG:-default}"
    echo "Cargo build jobs: ${CARGO_BUILD_JOBS:-default}"
    echo "UTC build time: $(date -u +%FT%TZ)"
    xcodebuild -version
    xcrun --sdk iphoneos --show-sdk-version
    (cd "$SOURCE_DIR" && rustc --version && cargo --version)
} > "$ARTIFACT_DIR/build-info.txt"

cd "$SOURCE_DIR"
xcodebuild \
    -project ios/Zed.xcodeproj \
    -scheme Zed \
    -configuration Debug \
    -sdk iphoneos \
    -destination 'generic/platform=iOS' \
    -derivedDataPath "$DERIVED_DATA" \
    ARCHS=arm64 \
    ONLY_ACTIVE_ARCH=YES \
    CODE_SIGNING_ALLOWED=NO \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGN_IDENTITY= \
    DEVELOPMENT_TEAM= \
    PRODUCT_BUNDLE_IDENTIFIER=io.github.yly25s.zed.ipad \
    CURRENT_PROJECT_VERSION="${GITHUB_RUN_NUMBER:-1}" \
    build 2>&1 | tee "$ARTIFACT_DIR/xcodebuild.log"

APP_PATH="$DERIVED_DATA/Build/Products/Debug-iphoneos/Zed.app"
test -d "$APP_PATH"
plutil -lint "$APP_PATH/Info.plist"
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_PATH/Info.plist")
xcrun lipo "$APP_PATH/$executable" -verify_arch arm64
{
    file "$APP_PATH/$executable"
    xcrun vtool -show-build "$APP_PATH/$executable"
    /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_PATH/Info.plist"
    /usr/libexec/PlistBuddy -c 'Print :MinimumOSVersion' "$APP_PATH/Info.plist"
    /usr/libexec/PlistBuddy -c 'Print :UIDeviceFamily' "$APP_PATH/Info.plist"
} | tee "$ARTIFACT_DIR/binary-info.txt"

# A temporary package directory prevents old Payload files entering a rerun.
PACKAGE_DIR=$(mktemp -d "$BUILD_ROOT/.work/package.XXXXXX")
trap 'rm -rf "$PACKAGE_DIR"' EXIT
mkdir "$PACKAGE_DIR/Payload"
ditto "$APP_PATH" "$PACKAGE_DIR/Payload/Zed.app"
(cd "$PACKAGE_DIR" && zip -qry Zed-iPadOS-unsigned.ipa Payload)
mv "$PACKAGE_DIR/Zed-iPadOS-unsigned.ipa" "$ARTIFACT_DIR/Zed-iPadOS-unsigned.ipa"
ditto -c -k --keepParent "$APP_PATH" "$ARTIFACT_DIR/Zed-iPadOS.app.zip"
unzip -tq "$ARTIFACT_DIR/Zed-iPadOS-unsigned.ipa"

# Ship the exact corresponding source and the small CI-only Cargo lock patch.
git archive --format=tar --prefix=zed-source/ HEAD | gzip > "$ARTIFACT_DIR/zed-source.tar.gz"
git diff --binary > "$ARTIFACT_DIR/source.patch"
cp LICENSE* "$ARTIFACT_DIR/"
cd "$ARTIFACT_DIR"
shasum -a 256 ./*.ipa ./*.zip ./*.tar.gz source.patch > SHA256SUMS

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
        echo '## Zed iPadOS build succeeded'
        echo
        echo '- Device: arm64 iPad, iPadOS 17 or newer.'
        echo '- `Zed-iPadOS-unsigned.ipa`: requires your own signing before installation.'
        echo '- `Zed-iPadOS.app.zip`: unsigned application bundle.'
        echo '- Source archive, CI patch, build metadata, licenses, and SHA-256 hashes are included.'
        echo '- Compilation and package checks passed; physical-device execution has not been tested.'
        echo
        echo '```'
        cat build-info.txt binary-info.txt SHA256SUMS
        echo '```'
    } >> "$GITHUB_STEP_SUMMARY"
fi
