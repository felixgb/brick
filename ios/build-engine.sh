#!/bin/sh
# Builds the Rust engine for the platform Xcode is building, as a static
# library the app links (see LIBRARY_SEARCH_PATHS in the project).
# Run by the "Build engine" phase; also works by hand: build-engine.sh iphonesimulator
set -eu
platform="${1:-${PLATFORM_NAME:-iphonesimulator}}"
case "$platform" in
    iphonesimulator) target=aarch64-apple-ios-sim ;;
    iphoneos) target=aarch64-apple-ios ;;
    *) echo "build-engine.sh: unknown platform $platform" >&2; exit 1 ;;
esac
cd "$(dirname "$0")/.."
# Xcode's environment (SDKROOT and friends) points at the iOS SDK, which breaks
# cargo's build scripts, which run on the Mac. Start from a clean one.
exec env -i HOME="$HOME" PATH="$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-26.0}" \
    cargo build --release -p brick-core --target "$target"
