## What's Changed

📦 Pre-built binaries for Linux (x86_64, aarch64, armv7).

### 🆕 Pre-built Binaries

v0.2.0까지는 macOS Apple Silicon만 지원했지만, 이번 릴리즈부터 6개 타겟의 pre-built tar.gz 제공:

- **macOS**: `aarch64-apple-darwin`
- **Linux glibc**: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
- **Linux musl** (Alpine, scratch container): `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
- **Linux 32-bit ARM**: `armv7-unknown-linux-gnueabihf` (Raspberry Pi 2/3)

`install.sh`도 platform/arch 자동 감지 + tar.gz 추출로 갱신됨.

### 🔧 Improvements

- 🛠 Release CI rewritten with `taiki-e/upload-rust-binary-action` matrix (6 parallel builds)
- 🔁 Homebrew tap formula auto-updated via heredoc regeneration on every release

### 🐛 Fixes

- 🔑 Homebrew tap auto-update — fix `TAP_GITHUB_TOKEN` permission flow that broke v0.2.0

### 📦 Install

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

> Other methods (Homebrew, cargo install, build from source, manual download): see [README](README.md#installation)

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.2.0...v0.2.1
