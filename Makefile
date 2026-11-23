# Brick — convenience targets. See CLAUDE.md / README.md for details.
#
#   make              build the Linux app (engine included)
#   make run          build and launch it
#   make test         engine tests
#   make help         list everything

QT_PREFIX  ?= $(HOME)/Qt/6.8.3/gcc_64
BUILD_DIR  ?= linux/build
GENERATOR  ?= Ninja
PREFIX     ?= /usr/local
CARGO      ?= cargo

GREENMAIL_IMAGE ?= greenmail/standalone:2.1.2
GREENMAIL_NAME  ?= brick-greenmail

.DEFAULT_GOAL := linux
.PHONY: help core linux configure run install test test-live greenmail-start greenmail-stop \
        headless headless-stop headless-restart headless-log shot macos fmt clippy clean distclean

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
	  awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ---- Engine ---------------------------------------------------------------

core: ## Build the Rust engine (release static lib)
	$(CARGO) build --release -p brick-core

test: ## Run all engine tests (live server test is skipped)
	$(CARGO) test -p brick-core

test-live: greenmail-start ## Run the live IMAP/SMTP test against a fresh greenmail container
	BRICK_TEST_SERVER=1 $(CARGO) test -p brick-core --test live_server -- --nocapture; \
	  status=$$?; docker rm -f $(GREENMAIL_NAME) >/dev/null; exit $$status

greenmail-start: ## Start a fresh greenmail container
	-@docker rm -f $(GREENMAIL_NAME) >/dev/null 2>&1
	docker run --rm -d --name $(GREENMAIL_NAME) \
	  -p 127.0.0.1:3143:3143 -p 127.0.0.1:3025:3025 \
	  -e GREENMAIL_OPTS='-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=test:secret@example.com' \
	  $(GREENMAIL_IMAGE)
	@sleep 3

greenmail-stop: ## Stop the greenmail container
	-docker rm -f $(GREENMAIL_NAME)

fmt: ## Format Rust code
	$(CARGO) fmt --all

clippy: ## Lint Rust code
	$(CARGO) clippy -p brick-core --all-targets

# ---- Linux app ------------------------------------------------------------

$(BUILD_DIR)/build.ninja $(BUILD_DIR)/Makefile:
	cmake -S linux -B $(BUILD_DIR) -G $(GENERATOR) -DCMAKE_PREFIX_PATH=$(QT_PREFIX) -DCMAKE_INSTALL_PREFIX=$(PREFIX)

configure: ## (Re)configure the CMake build
	cmake -S linux -B $(BUILD_DIR) -G $(GENERATOR) -DCMAKE_PREFIX_PATH=$(QT_PREFIX) -DCMAKE_INSTALL_PREFIX=$(PREFIX)

linux: | $(if $(filter Ninja,$(GENERATOR)),$(BUILD_DIR)/build.ninja,$(BUILD_DIR)/Makefile) ## Build the Linux app (also builds the engine)
	cmake --build $(BUILD_DIR)

run: linux ## Build and run the Linux app
	./$(BUILD_DIR)/brick

install: linux ## Install the Linux app under PREFIX
	cmake --install $(BUILD_DIR)

# ---- Headless debugging ---------------------------------------------------

headless: linux ## Start the app headless with demo mail and items
	linux/tools/run-headless.sh restart --seed
	linux/tools/seed-items.py

headless-stop: ## Stop the headless app
	linux/tools/run-headless.sh stop

headless-restart: linux ## Rebuild and restart the headless app
	linux/tools/run-headless.sh restart

headless-log: ## Show the headless app's log
	linux/tools/run-headless.sh log

shot: ## Screenshot the headless app to /tmp/brick.png (override with SHOT=...)
	linux/tools/brickctl.py shot $(or $(SHOT),/tmp/brick.png)

# ---- macOS app ------------------------------------------------------------

macos: ## Build macos/Brick.app (macOS only)
	./macos/build-app.sh

# ---- Cleanup --------------------------------------------------------------

clean: ## Remove the Linux build dir and cargo artifacts
	rm -rf $(BUILD_DIR)
	$(CARGO) clean

distclean: clean headless-stop ## Also remove macOS build output
	rm -rf macos/.build macos/Brick.app
