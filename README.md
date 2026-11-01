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
app/       Flutter app (all platforms)            ← created in T-0001
engine/    Rust workspace (engine crates)         ← created in T-0001
assets/    Fonts, library stickers, sounds (+ licences manifest)
scripts/   Tooling
```

## Build from source

Instructions are added by ticket T-0001 (toolchain versions for Rust, Flutter, Android NDK, Xcode).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
