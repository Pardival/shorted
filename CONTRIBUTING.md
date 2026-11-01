# Contributing to Shorted

Thanks for helping! Shorted is early: please open an issue before starting work.

## How to contribute

1. Pick an open issue or propose a change in a new issue first.
2. Small focused PR, Conventional Commits (`feat(audio): …`).
3. Run formatters, linters and tests.
4. Fill the PR template, including the security checklist.

## Ground rules

- Fluidity is the product: changes must not regress performance.
- No dependency with a licence incompatible with app-store distribution (no GPL-only code in shipped binaries).
- No telemetry, analytics or network calls beyond model downloads and user-initiated AI calls.
- All user-facing text goes through the localisation files (English source, plus French, Spanish, Portuguese).

## Code of conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
