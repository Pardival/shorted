# Shorted

**Turn your clips into a polished vertical short in minutes.**
Shorted is a free, open-source video editor for iOS, Android, macOS, Windows and Linux, made for explanatory
shorts: a day in your life, a reflection, a mini-vlog, an explainer.

- **Clips** — drop your videos and photos; *Smart arrange* removes dead time and orders them, or you order them yourself.
- **Audio** — keep the clip's own sound, record a voice-over over the video, add an external voice or music.
- **Captions** — speech is transcribed on your device; word-by-word captions with emphasis, placed automatically.
- **Enhance** — text, images, GIFs, stickers and transitions you drag onto the video, animate, and move along a path you draw.
- **Export** — 9:16 MP4 up to 4K, identical to the preview.

Local-first: your videos never leave your device. No account. AI assistance is optional, with your own API key.

> **Status:** pre-alpha — implementation starting.
> **Licence:** to be decided.

## Repository layout

```
app/       Flutter app (all platforms)
engine/    Rust workspace (engine crates)
assets/    Fonts, library stickers, sounds (+ licences manifest)
scripts/   Tooling
```

## Build from source

| Tool | Version |
| --- | --- |
| Rust | 1.99.1 (pinned in `rust-toolchain.toml`) |
| Flutter | 3.47.9 stable, Dart 3.13.7 (pinned in `.flutter-version`) |
| Xcode (iOS, macOS) | 26.3, with CocoaPods 1.17 |
| Android | Android SDK and NDK, minimum API 29 |
| Windows | Visual Studio 2022 with the "Desktop development with C++" workload |
| Linux | `clang cmake ninja-build pkg-config libgtk-3-dev` |

```bash
cd engine && cargo build && cargo test
```

```bash
cd app && flutter pub get && flutter run -d macos
```

The first build compiles the engine through the `rust_builder` plugin. After changing the API in
`engine/crates/shorted_ffi/src/api`, install `flutter_rust_bridge_codegen` 2.13.0 and run `scripts/codegen.sh`.

Replace `macos` with `windows`, `linux`, or the id of a connected device (`flutter devices`).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
