.PHONY: build build-universal test smoke release-check install-artifacts workflows-pack plugin-bundle release-artifacts artifacts codebasezip install release release-dry-run

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

release-check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	python3 -m compileall -q scripts
	python3 scripts/validate_workflow_catalog.py --offline
	python3 scripts/test_validate_workflow_catalog.py
	python3 scripts/build_workflow_pack.py --out /tmp/rzn-workflow-pack-test
	python3 scripts/test_release_archive_safety.py

install-artifacts: build-universal
	python3 ./scripts/build_release_artifacts.py --platform $(PLATFORM)

workflows-pack: install-artifacts

plugin-bundle: build-universal
	./scripts/package_plugin.sh

release-artifacts: install-artifacts plugin-bundle

artifacts: release-artifacts

codebasezip:
	./scripts/codebase_zip.sh

install: install-artifacts
	./scripts/install_rzn_phone.sh --stage "$(RELEASE_DIR)/package" --update-source "$(abspath $(RELEASE_DIR))"

release:
	@test -n "$(NEXT_VERSION)" || (echo "usage: make release NEXT_VERSION=0.1.1" >&2; exit 1)
	python3 ./scripts/release.py --version "$(NEXT_VERSION)"

release-dry-run:
	@test -n "$(NEXT_VERSION)" || (echo "usage: make release-dry-run NEXT_VERSION=0.1.1" >&2; exit 1)
	python3 ./scripts/release.py --version "$(NEXT_VERSION)" --dry-run
