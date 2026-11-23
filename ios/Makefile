# Brick for iPhone. See ../CLAUDE.md.
PROJECT   := Brick.xcodeproj
SCHEME    := Brick
BUNDLE_ID := st.mm.fgb.brick
BUILD_DIR := build

# Override on the command line, e.g. `make simulator SIMULATOR="iPhone 18 Pro"`.
SIMULATOR ?= iPhone 17
# `xcrun devicectl list devices` shows the identifiers of connected devices.
DEVICE    ?= 00008101-001478D20EF1003A

XCODEBUILD := xcodebuild -project $(PROJECT) -scheme $(SCHEME) -derivedDataPath $(BUILD_DIR) -quiet
PRODUCTS   := $(BUILD_DIR)/Build/Products
DEVICE_HUB := $(shell xcode-select -p)/../Applications/DeviceHub.app

.PHONY: build simulator install clean

# Compile for the simulator (the engine is built by the project's "Build engine" phase).
build:
	$(XCODEBUILD) -destination 'platform=iOS Simulator,name=$(SIMULATOR)' build

# Build, boot the simulator, install and launch.
simulator: build
	xcrun simctl bootstatus '$(SIMULATOR)' -b
	open '$(DEVICE_HUB)'
	xcrun simctl install '$(SIMULATOR)' $(PRODUCTS)/Debug-iphonesimulator/Brick.app
	xcrun simctl launch --terminate-running-process '$(SIMULATOR)' $(BUNDLE_ID)

# Build and install on the phone, which must be connected and unlocked.
install:
	$(XCODEBUILD) -destination 'id=$(DEVICE)' -allowProvisioningUpdates build
	xcrun devicectl device install app --device $(DEVICE) $(PRODUCTS)/Debug-iphoneos/Brick.app
	@xcrun devicectl device process launch --device $(DEVICE) --terminate-existing $(BUNDLE_ID) \
		|| echo "Installed, but could not open the app. Unlock the phone and open Brick there."

clean:
	rm -rf $(BUILD_DIR)
