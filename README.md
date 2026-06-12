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

> 当前实现进度见 [STATUS.md](STATUS.md)（唯一事实来源）。

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

## 📖 Documentation

- [Status](STATUS.md) - 当前真实进度与已知技术债
- [Architecture](ARCHITECTURE.md) - System design and architecture
- [Plan](PLAN.md) - Roadmap (Phase 1-7)
- [Contributing](CONTRIBUTING.md) - Development guidelines
- [Quick Start](QUICKSTART.md) - Installation and usage guide

## 🏗️ Project Structure

```
deploykeys-desktop/
├── crates/
│   ├── deploykeys-core/      # Core business logic (native, no UI deps)
│   ├── deploykeys-gui/       # Tauri 2 native host (binary: `deploykeys`)
│   └── deploykeys-ui/        # Leptos CSR frontend, built to wasm by Trunk
├── migrations/            # Database migrations (sqlx::migrate!)
├── tools/                 # Pinned Tailwind v4 standalone binary + installer
├── docs/archive/          # Superseded phase reports (do not cite)
└── .github/workflows/     # CI (fmt, clippy, test, audit)
```

The Tauri host (`deploykeys-gui`) opens the database and exposes a small IPC
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

## 🛡️ Security

- Tokens stored in system keychain (never in database); SQLite keeps reference keys only
- OAuth Device Flow per RFC 8628 (interval, slow_down, expiry honored)
- Automatic log sanitization (`ghu_/gho_/ghs_/ghp_/github_pat_` tokens, auth headers, JSON token fields)
- Private keys created atomically with mode 0600; partial failures are cleaned up
- No modifications to `~/.ssh/config` or `~/.gitconfig`

## 📊 Status

详见 [STATUS.md](STATUS.md)。概要：

- ✅ Phase 1: 项目骨架与数据层
- ⚙️ Phase 2: GitHub 认证（设备流/账号持久化完成；Installations 同步与 token 刷新未完成）
- 📋 Phase 3-7: Key 生成绑定流程部分提前实现，其余按 PLAN.md 推进

## 🤝 Contributing

We welcome contributions! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

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
