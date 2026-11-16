# Aurelius

A calm, local workspace where you run your own AI team (a CEO, a CTO, employees and departments) on top of [Claude Code](https://claude.com/claude-code). Everything runs and is stored on your own computer.

Aurelius has a desktop app (Windows and macOS), a mobile app (iPhone and Android) and a server that keeps all state locally in SQLite.

## Requirements

[Claude Code](https://claude.com/claude-code), installed and logged in.

## Install

**Releases:** download the latest build from [GitHub Releases](https://github.com/Segrusa/Aurelius/releases). The Windows release is a signed zip, and the app updates itself from there. On macOS, build from source and install with `scripts/install-mac.sh`.

**From source:** you need Rust via [rustup](https://rustup.rs) (the pinned toolchain installs itself) and Node.js (see `.node-version`). Linux and macOS also need CMake, macOS the Xcode command line tools, and Windows the Visual Studio 2022 Build Tools with the C++ workload.

On Linux and macOS, `scripts/install-linux.sh` and `scripts/install-mac.sh` build and install everything, and `scripts/install-production.ps1` does so on Windows. On Windows, `scripts\install-production.ps1 -InstallDeps` first installs whatever is missing (Git, Rust, the Build Tools, Node.js) with winget; `scripts\windows-deps.ps1` only checks.

The app starts the server for you.

## Mobile app

The Flutter app in `clients/mobile` connects to Aurelius on your own PC over Tailscale. See [docs/mobile.md](docs/mobile.md).

## Contributing and security

Run `cargo test` and, for the mobile app, `flutter test` (in `clients/mobile`) before opening a pull request.
Report security problems through GitHub's [private vulnerability reporting](https://github.com/Segrusa/Aurelius/security/advisories/new), not in a public issue.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
