.PHONY: help build test fmt clippy clean db-clean run dev ui-build install check audit db-setup watch docs coverage

# Default target
help:
	@echo "DeployKeys Desktop - Development Commands"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build      - Bundle the release desktop app (Tauri + Trunk frontend)"
	@echo "  dev        - Build the native crates in debug"
	@echo "  run        - Run the app in development (Tauri + Trunk dev server)"
	@echo "  ui-build   - Build the Leptos/wasm frontend with Trunk"
	@echo "  test       - Run all tests (native crates)"
	@echo "  fmt        - Format code"
	@echo "  clippy     - Run Clippy linter"
	@echo "  check      - Run all checks (fmt + clippy + test)"
	@echo "  audit      - Check for security vulnerabilities"
	@echo "  clean      - Clean build artifacts (keeps deploykeys.db)"
	@echo "  db-setup   - Create the sqlx compile-time check database"
	@echo "  db-clean   - Remove the sqlx compile-time check database"
	@echo "  install    - Bundle the app and print install instructions"
	@echo "  watch      - Auto-run clippy + test on change (native crates)"
	@echo ""

# Bundle the release desktop app. Tauri runs the configured beforeBuildCommand
# (trunk build --release) to produce the frontend, then bundles the binary.
build:
	@echo "Bundling release desktop app..."
	cargo tauri build

# Build the native crates in debug (core + Tauri backend). The wasm-only UI
# crate is excluded via default-members, so this never tries to build it natively.
dev:
	@echo "Building native crates (debug)..."
	cargo build

# Run the app in development. Tauri starts the Trunk dev server (beforeDevCommand)
# and opens the desktop window pointed at it with hot reload.
run:
	@echo "Running application (Tauri dev)..."
	cargo tauri dev

# Build just the Leptos/wasm frontend with Trunk.
ui-build:
	@echo "Building frontend (Trunk)..."
	cd crates/deploykeys-ui && trunk build --release

# Run all tests. The wasm-only UI crate is excluded via default-members.
test:
	@echo "Running tests..."
	cargo test

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt --all

# Run Clippy
clippy:
	@echo "Running Clippy..."
	cargo clippy --workspace --all-targets -- -D warnings

# Run all checks
check: fmt clippy test
	@echo "All checks passed! ✓"

# Security audit
audit:
	@echo "Checking for vulnerabilities..."
	cargo audit

# Clean build artifacts. deploykeys.db is kept because sqlx compile-time
# query checks need it; use db-clean to remove it explicitly.
clean:
	@echo "Cleaning..."
	cargo clean

# Install application. A Tauri app needs its bundled webview assets, so the
# right artifact is the bundle produced by `cargo tauri build` (see the `build`
# target) — not `cargo install`, which would skip the frontend. On macOS, copy
# the resulting .app from target/release/bundle/macos/ into /Applications.
install: build
	@echo "Build complete. Copy the bundle from target/release/bundle/ to install."
	@echo "  macOS: cp -R 'target/release/bundle/macos/DeployKeys Desktop.app' /Applications/"

# Create the database used by sqlx compile-time query checks
db-setup:
	@echo "Setting up database..."
	@if [ ! -f .env ]; then cp .env.example .env; fi
	rm -f deploykeys.db deploykeys.db-shm deploykeys.db-wal
	@for m in migrations/*.sql; do echo "  applying $$m"; sqlite3 deploykeys.db < "$$m"; done
	@echo "Database created at deploykeys.db"

# Remove the compile-time check database (build will fail until db-setup)
db-clean:
	rm -f deploykeys.db deploykeys.db-shm deploykeys.db-wal

# Watch and auto-rebuild. `cargo tauri dev` already hot-reloads: Trunk watches
# the frontend and rebuilds the wasm, and Tauri rebuilds the native host on
# change. So development watching is just `make run`. Use this target for a
# native-crate-only check loop (no window) when iterating on core/host logic.
watch:
	cargo watch -x 'clippy --workspace --all-targets -- -D warnings' -x test

# Generate documentation
docs:
	@echo "Generating documentation..."
	cargo doc --no-deps --workspace --open

# Coverage report (requires cargo-tarpaulin)
coverage:
	@echo "Generating coverage report..."
	cargo tarpaulin --workspace --out Html
