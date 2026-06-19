# DeployKeys Desktop

> **Target-based GitHub Deploy Key Manager**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

A secure, user-friendly desktop application for managing GitHub Deploy Keys with target-based key generation.

## ✨ Features

- 🔐 **Target-based Key Generation** - Generate SSH keys on local or remote servers
- 🔑 **Multi-Algorithm Support** - Ed25519, RSA 2048/4096, ECDSA P-256/384/521
- 🔒 **Private Key Isolation** - Keys never leave the target environment
- ✅ **Drift Detection** - Validation and status monitoring
- 🔐 **Secure Credential Storage** - Uses system keychain (macOS/Linux)
- 🌐 **Internationalization** - 31 interface languages with runtime switching and Arabic RTL support

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+ ([Install Rust](https://rustup.rs))
- macOS 10.15+ or Linux (x86_64)
- `sqlite3` CLI (for the compile-time check database)
- The `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev) (builds the Leptos/wasm frontend): `cargo install trunk`
- The Tauri CLI (drives dev/build): `cargo install tauri-cli --version "^2"` (provides `cargo tauri`)

> Tailwind needs no Node toolchain: a pinned standalone Tailwind v4 binary lives
> in `tools/` and is run by Trunk's pre-build hook. Fetch it once with
> `tools/install-tailwind.sh` if it is missing.

### Installation

```bash
git clone <repository-url>
cd deploykeys-desktop

# Create the database used by sqlx compile-time query checks
make db-setup

# Run the app in development (Tauri opens a window onto the Trunk dev server)
make run
```

### Development Setup

```bash
# Copy environment template (db-setup does this automatically)
cp .env.example .env

# Set up the compile-time check database
make db-setup

# Run all checks (fmt + clippy -D warnings + test) on the native crates
make check
```

## 🏗️ Project Structure

```
deploykeys-desktop/
├── crates/
│   ├── deploykeys-core/      # Core business logic (native, no UI deps)
│   ├── deploykeys-app/       # Tauri 2 native host (binary: `deploykeys`)
│   └── deploykeys-ui/        # Leptos CSR frontend, built to wasm by Trunk
├── migrations/            # Database migrations (sqlx::migrate!)
├── tools/                 # Pinned Tailwind v4 standalone binary + installer
└── .github/workflows/     # CI (fmt, clippy, test, audit)
```

The Tauri host (`deploykeys-app`) opens the database and exposes a small IPC
command surface that bridges to `deploykeys-core`. The webview (`deploykeys-ui`)
is plain CSR wasm and never depends on `deploykeys-core` — it calls the IPC
commands and receives purpose-built DTOs, so secrets (keyring references,
tokens) never cross the IPC boundary. The UI crate is wasm-only and excluded
from the workspace `default-members`, so a bare `cargo build`/`cargo test` at
the root only touches the native crates.

## 🔧 Development Commands

```bash
make run        # Run in development (Tauri + Trunk dev server, hot reload)
make build      # Bundle the release desktop app (Tauri + Trunk frontend)
make dev        # Build the native crates in debug
make ui-build   # Build just the Leptos/wasm frontend with Trunk
make test       # Run all tests (native crates)
make fmt        # Format code
make clippy     # Run linter (deny warnings)
make check      # Run all checks (fmt + clippy + test)
make audit      # Security audit
make db-setup   # (Re)create the sqlx compile-time check database
```

## 🌐 Internationalization

The UI ships in **31 interface languages** and switches at runtime — no restart
needed. The preference is persisted via the same `app_settings` store as the
rest of the app, and a legacy value (`zh`) is still recognized as Simplified
Chinese after an upgrade.

**Supported languages:** English, 简体中文, 繁體中文, 日本語, Español, العربية,
Português, Bahasa Indonesia, 한국어, Français, Deutsch, Italiano, Русский, ไทย,
Tiếng Việt, Türkçe, Polski, Nederlands, Svenska, Dansk, Norsk, Suomi, Čeština,
Slovenčina, Română, Українська, Magyar, हिन्दी, বাংলা, Bahasa Melayu, Filipino.

The language can be changed from three places, all backed by the same shared,
searchable picker:

- The **globe button** in the top bar
- **Settings → Language**
- The **command palette** (`⌘K` / `Ctrl+K`) → "Change language"

Arabic flips the whole document to right-to-left (`<html dir="rtl">`); every
other language is left-to-right.

Translations live in a single inline table at
`crates/deploykeys-ui/src/i18n.rs` (kept wasm-friendly rather than loaded from
external files). English is the baseline and always complete; any missing key
falls back to English. A key-set consistency test enforces that **every**
language table exposes the exact same keys as English with no duplicates, so a
partially-translated table can't silently ship. Adding a language is a
three-step change:

1. Add a `Locale` variant + a row in `Locale::ALL` (and the metadata methods).
2. Add a translation `const` table whose keys match `EN` exactly.
3. Wire it into `lookup()` and the `table_for` test helper.

Then `cargo test -p deploykeys-ui --bin deploykeys-ui i18n::` confirms parity.

## 🛡️ Security

- Tokens stored in system keychain (never in database); SQLite keeps reference keys only
- OAuth Device Flow per RFC 8628 (interval, slow_down, expiry honored)
- Automatic log sanitization (`ghu_/gho_/ghs_/ghp_/github_pat_` tokens, auth headers, JSON token fields)
- Private keys created atomically with mode 0600; partial failures are cleaned up
- No modifications to `~/.ssh/config` or `~/.gitconfig`

## 📄 License

This project is licensed under the MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

Built with:
- [Rust](https://www.rust-lang.org/) - Programming language
- [Tauri 2](https://tauri.app/) - Desktop shell (native host + webview)
- [Leptos](https://leptos.dev/) - Rust/wasm UI framework (CSR frontend)
- [Trunk](https://trunkrs.dev/) - wasm build tool
- [Tailwind CSS](https://tailwindcss.com/) - Utility-first styling (standalone v4 binary)
- [SQLx](https://github.com/launchbadge/sqlx) - Async SQL toolkit
- [Tokio](https://tokio.rs/) - Async runtime
