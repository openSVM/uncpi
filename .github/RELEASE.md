# Release Process

## Setup (One-time)

1. **Get a crates.io API token**:
   - Go to https://crates.io/settings/tokens
   - Click "New Token"
   - Give it a name like "uncpi-github-actions"
   - Copy the token

2. **Add token to GitHub Secrets**:
   - Go to https://github.com/openSVM/uncpi/settings/secrets/actions
   - Click "New repository secret"
   - Name: `CARGO_REGISTRY_TOKEN`
   - Value: Paste your crates.io token
   - Click "Add secret"

## Creating a Release

### Method 1: Using GitHub CLI (Recommended)

```bash
# Make sure you're on master and up to date
git checkout master
git pull

# Create and push a new tag
git tag v0.1.0
git push origin v0.1.0
```

### Method 2: Using GitHub Web UI

1. Go to https://github.com/openSVM/uncpi/releases/new
2. Choose or create a tag (e.g., `v0.1.0`)
3. Click "Generate release notes"
4. Publish release

## What Happens Automatically

When you push a tag starting with `v`:

1. ✅ **CI Tests Run** - Ensures code quality
2. ✅ **Multi-platform Builds** - Creates optimized binaries for:
   - Linux (x86_64-gnu, x86_64-musl, aarch64)
   - macOS (x86_64, aarch64/M1)
   - Windows (x86_64)
3. ✅ **GitHub Release** - Creates release with binaries attached
4. ✅ **Crates.io Publish** - Publishes to crates.io registry

## Version Bumping

Before creating a release, update the version in `Cargo.toml`:

```toml
[package]
version = "0.1.1"  # Bump this
```

Then commit:
```bash
git add Cargo.toml
git commit -m "Bump version to 0.1.1"
git push
```

## Yanking a Release

If you need to yank a bad release from crates.io:

```bash
cargo yank --version 0.1.0 uncpi
```

## Pre-release Testing

Before creating a tag, test the release build locally:

```bash
cargo build --release
./target/release/uncpi --help
```

Test publishing (dry run):
```bash
cargo publish --dry-run
```
