# Versioning Strategy

MCP-Muse uses [Calendar Versioning (CalVer)](https://calver.org/) to clearly communicate when a release was published.

## Version Format

**`YYYY.MM.PATCH`**

- **YYYY**: Full 4-digit year (e.g., 2025)
- **MM**: Month without leading zero (e.g., 1, 2, ..., 11, 12)
- **PATCH**: Incremental patch number for multiple releases in the same month (0, 1, 2, ...)

### Examples

- `2025.11.0` - First release in November 2025
- `2025.11.1` - Second release in November 2025 (bug fix or minor update)
- `2025.12.0` - First release in December 2025
- `2026.1.0` - First release in January 2026

## Why CalVer?

MCP-Muse uses CalVer instead of Semantic Versioning (SemVer) for several reasons:

1. **Temporal Context**: Users immediately know how recent a version is
2. **Continuous Development**: The project evolves continuously without strict API stability guarantees
3. **Release Cadence**: Natural fit for regular releases tied to development activity
4. **Simplicity**: Easier to understand for end users ("this was released in November 2025")

## Automatic Release Process

Releases are **fully automated** via GitHub Actions:

### How It Works

1. **Developer merges a PR to `main`** → Triggers the auto-tag workflow
2. **Auto-tag workflow**:
   - Calculates the next CalVer version based on current date
   - Checks if any releases exist for the current year-month
   - If no releases exist: Creates `v2025.11.0`
   - If releases exist: Increments patch number (e.g., `v2025.11.1`)
   - Updates `Cargo.toml` with the new version
   - Commits the version bump to `main`
   - Creates and pushes the git tag
3. **Tag push triggers release workflow**:
   - Builds binaries for all platforms (Linux, macOS, Windows)
   - Creates GitHub Release with binaries attached
   - Publishes to [crates.io](https://crates.io/crates/mcp-muse)
   - Generates automated release notes

### Skip a Release

To merge changes to `main` without triggering a release, include `[skip-release]` in your commit message:

```bash
git commit -m "docs: update README [skip-release]"
```

This is useful for documentation-only changes or when you want to batch multiple changes before releasing.

## Installing Specific Versions

### Latest Version (Recommended)

```bash
cargo install mcp-muse
```

### Specific Version

```bash
cargo install mcp-muse --version 2025.11.0
```

### From Git Tag

```bash
cargo install --git https://github.com/alextrzyna/mcp-muse --tag v2025.11.0
```

## Version History

All releases are available on the [GitHub Releases](https://github.com/alextrzyna/mcp-muse/releases) page with:
- Pre-built binaries for all platforms
- Automated changelog
- Full release notes

## Breaking Changes

While we use CalVer, we still care about compatibility:

- **Minor updates** (patch increment within a month) should be safe to upgrade
- **Major changes** will be clearly documented in release notes
- **Breaking changes** will be communicated in advance when possible

For stability-critical deployments, we recommend pinning to a specific version in your dependencies.

## Developer Notes

### Manual Tagging (If Needed)

While releases are normally automatic, you can manually create a release:

```bash
# Calculate next version (example for December 2025)
git tag -a v2025.12.0 -m "Release v2025.12.0"
git push origin v2025.12.0
```

The release workflow will automatically build and publish when it detects the new tag.

### Updating Version in Development

The version in `Cargo.toml` is automatically updated by the release process. When developing locally, the version number may be slightly behind the latest release until the next merge to `main`.

## Questions or Issues?

If you have questions about versioning or encounter issues with releases:

1. Check the [GitHub Releases](https://github.com/alextrzyna/mcp-muse/releases) page
2. Review the [GitHub Actions](https://github.com/alextrzyna/mcp-muse/actions) workflow runs
3. [Open an issue](https://github.com/alextrzyna/mcp-muse/issues) if you need help
