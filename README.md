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

### Installation

```bash
git clone <repository-url>
cd deploykeys-desktop

# Create the database used by sqlx compile-time query checks
make db-setup

# Build and run
make run
```

### Development Setup

```bash
# Copy environment template (db-setup does this automatically)
cp .env.example .env

# Set up the compile-time check database
make db-setup

# Run all checks (fmt + clippy -D warnings + test)
make check

# Run in watch mode (requires cargo-watch)
make watch
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
│   ├── deploykeys-core/      # Core business logic
│   └── deploykeys-gui/       # Iced GUI application
├── migrations/            # Database migrations (sqlx::migrate!)
├── docs/archive/          # Superseded phase reports (do not cite)
└── .github/workflows/     # CI (fmt, clippy, test, audit)
```

## 🔧 Development Commands

```bash
make build      # Build release binary
make test       # Run all tests
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
- [Iced](https://iced.rs/) - Cross-platform GUI framework
- [SQLx](https://github.com/launchbadge/sqlx) - Async SQL toolkit
- [Tokio](https://tokio.rs/) - Async runtime
