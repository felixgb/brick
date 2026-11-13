#!/bin/sh
# Builds Brick.app next to this script. Needs Rust and Xcode 15 or later.
set -eu
cd "$(dirname "$0")"

(cd .. && cargo build --release -p brick-core)
swift build -c release

APP=Brick.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$(swift build -c release --show-bin-path)/Brick" "$APP/Contents/MacOS/Brick"
cp Info.plist "$APP/Contents/Info.plist"

# Ad-hoc signature: enough to run on this Mac and to use the Keychain.
codesign --force --sign - "$APP"
echo "Built $(pwd)/$APP"
