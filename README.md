# Aurelius

A calm, local workspace where you run your own AI team (a CEO, a CTO, employees and departments) on top of [Claude Code](https://claude.com/claude-code). Everything runs and is stored on your own computer.

Aurelius has a desktop app (Windows and macOS), a mobile app (iPhone and Android) and a server that keeps all state locally in SQLite.

## Requirements

[Claude Code](https://claude.com/claude-code), installed and logged in.

## Install

**Releases:** download the latest build from [GitHub Releases](https://github.com/opteon-sdc/aurelius/releases). The Windows release is a signed zip, and the app updates itself from there. On macOS, build from source and install with `scripts/install-mac.sh`.

**From source:** you need Rust via [rustup](https://rustup.rs) (the pinned toolchain installs itself) and CMake. Windows also needs the Visual Studio 2022 Build Tools with the C++ workload, and macOS needs the Xcode command line tools.

```bash
cargo build --release -p aurelius-desktop -p aurelius-daemon -p aurelius-client
./target/release/aurelius
```

The app starts the server for you.

## Mobile app

The Flutter app in `clients/mobile` connects to Aurelius on your own PC over Tailscale. See [docs/mobile.md](docs/mobile.md).

## Contributing and security

Run `cargo test` and, for the mobile app, `flutter test` (in `clients/mobile`) before opening a pull request.
Report security problems through GitHub's [private vulnerability reporting](https://github.com/opteon-sdc/aurelius/security/advisories/new), not in a public issue.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
