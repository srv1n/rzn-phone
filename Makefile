.PHONY: build build-universal test smoke install-artifacts workflows-pack plugin-bundle release-artifacts artifacts install release release-dry-run

VERSION := $(shell python3 ./scripts/release_common.py current-version)
PLATFORM := macos_universal
RELEASE_DIR := dist/releases/rzn-phone/$(VERSION)/$(PLATFORM)

build:
	cargo build -p rzn_phone_worker --release

build-universal:
	./scripts/build_universal.sh

test:
	cargo test -p rzn_phone_worker

smoke:
	./scripts/run_smoke.sh

install-artifacts: build-universal
	python3 ./scripts/build_release_artifacts.py --platform $(PLATFORM)

workflows-pack: install-artifacts

plugin-bundle: build-universal
	./scripts/package_plugin.sh

release-artifacts: install-artifacts plugin-bundle

artifacts: release-artifacts

install: install-artifacts
	./scripts/install_rzn_phone.sh --stage "$(RELEASE_DIR)/package" --update-source "$(abspath $(RELEASE_DIR))"

release:
	@test -n "$(NEXT_VERSION)" || (echo "usage: make release NEXT_VERSION=0.1.1" >&2; exit 1)
	python3 ./scripts/release.py --version "$(NEXT_VERSION)"

release-dry-run:
	@test -n "$(NEXT_VERSION)" || (echo "usage: make release-dry-run NEXT_VERSION=0.1.1" >&2; exit 1)
	python3 ./scripts/release.py --version "$(NEXT_VERSION)" --dry-run
