# Contributing to Kaku

## Setup

```bash
# Clone the repository
git clone https://github.com/tw93/Kaku.git
cd Kaku

# Install required tools (cargo-nextest, nightly rustfmt)
make install-tools

# Install pre-commit hook (format + test before each commit)
make install-hooks
```

## Development

| Command | Purpose |
|---------|---------|
| `make fmt` | Auto-format code (requires nightly) |
| `make fmt-check` | Check formatting without modifying files |
| `make check` | Compile check, catch type/syntax errors |
| `make test` | Run unit tests |
| `make build` | Compile binaries (no app bundle) |
| `make app` | Debug build → `dist/Kaku.app` (fastest) |

**Recommended workflow:**

```bash
make fmt        # format first
make check      # verify it compiles
make test       # run tests
make app        # build and run when needed
```

## Build Release

```bash
# Build application and DMG (release, native)
./scripts/build.sh
# Outputs: dist/Kaku.app and dist/Kaku.dmg
```

## Pull Requests

1. Fork and create a branch from `main`
2. Make changes
3. Run `make fmt && make check && make test`
4. Commit and push
5. Open PR targeting `main`

CI runs format check → unit tests → cargo check → universal build validation in order.
