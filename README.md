# Aurelius

A calm workspace where you and your colleagues run your own AI team (a CEO, a tech lead, and the employees who report to them) on top of [Claude Code](https://claude.com/claude-code). Everything runs and is stored on a server of your own: your computer, or one machine the board members reach over Tailscale.

Aurelius has a server that keeps all state in SQLite, a desktop app (Windows and macOS), the same app in a browser, and a mobile app (iPhone and Android). Each board member signs in with their own name and connects their own Claude login.

What has been decided about how it works is in [docs/decisions.md](docs/decisions.md).

## Requirements

[Claude Code](https://claude.com/claude-code), installed and logged in.

## Install

**Releases:** download the latest build from [GitHub Releases](https://github.com/Segrusa/Aurelius/releases). The Windows release is a signed zip, and the app updates itself from there. On macOS, build from source and install with `scripts/install-mac.sh`.

**From source:** you need Rust via [rustup](https://rustup.rs) (the pinned toolchain installs itself) and CMake. Windows also needs the Visual Studio 2022 Build Tools with the C++ workload, and macOS needs the Xcode command line tools.

On Linux and macOS, `scripts/install-linux.sh` and `scripts/install-mac.sh` build and install everything, and `scripts/install-production.ps1` does so on Windows. They also need Node.js (see `.node-version`).

The app starts the server for you.

## Mobile app

The Flutter app in `clients/mobile` connects to your Aurelius server over Tailscale. See [docs/mobile.md](docs/mobile.md).

## Contributing and security

Run `cargo test` and, for the mobile app, `flutter test` (in `clients/mobile`) before opening a pull request.
Report security problems through GitHub's [private vulnerability reporting](https://github.com/Segrusa/Aurelius/security/advisories/new), not in a public issue.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
