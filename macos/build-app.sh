#!/bin/sh
# Builds Brick.app next to this script. Needs Rust and Xcode 15 or later.
set -eu
cd "$(dirname "$0")"

# Match the Swift package's minimum macOS so the linker doesn't warn about every object.
(cd .. && MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --release -p brick-core)
swift build -c release

APP=Brick.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$(swift build -c release --show-bin-path)/Brick" "$APP/Contents/MacOS/Brick"
cp Info.plist "$APP/Contents/Info.plist"
mkdir -p "$APP/Contents/Resources"
cp AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

# Ad-hoc signature: enough to run on this Mac.
codesign --force --sign - "$APP"
echo "Built $(pwd)/$APP"
