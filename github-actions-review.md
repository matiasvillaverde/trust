# GitHub Actions Review: Automated Release Workflow

This document provides an overview of the automated release workflow implemented for the Trust project.

## Overview

The Trust project now includes an automated release process that triggers when the version is bumped in `Cargo.toml`. This eliminates manual release steps and ensures consistent, reproducible releases across all supported platforms.

## Workflow Files

### 1. `.github/workflows/release-on-version-bump.yaml`

**Purpose**: Automatically creates GitHub releases when version changes are detected in `Cargo.toml`

**Trigger**: 
- Push to `main` branch
- Changes to `Cargo.toml` file

**Process**:
1. **Version Detection**: Compares current version with previous commit
2. **Release Creation**: Creates GitHub release with appropriate tag
3. **Cross-Platform Builds**: Builds binaries for all supported platforms
4. **Asset Upload**: Uploads platform-specific archives to the release

### 2. `.github/workflows/release.yaml` (Existing)

**Purpose**: Manual release process triggered by git tags
**Status**: Remains unchanged for manual releases if needed

## Workflow Jobs

### Job 1: `check-version`
- **Runs on**: `ubuntu-latest`
- **Purpose**: Detect if version has changed between commits
- **Outputs**: 
  - `version_changed`: Boolean indicating if version was updated
  - `new_version`: The new version number
- **Process**:
  1. Fetches current and previous commit
  2. Extracts version from `Cargo.toml` in both commits
  3. Compares versions and sets outputs

### Job 2: `create-release`
- **Runs on**: `ubuntu-latest`
- **Depends on**: `check-version`
- **Condition**: Only runs if version changed
- **Purpose**: Create the GitHub release
- **Process**:
  1. Creates release with tag `v{version}`
  2. Sets release name to `Release v{version}`
  3. Provides upload URL for subsequent jobs

### Job 3: `build-and-upload`
- **Runs on**: Platform-specific (Ubuntu for Linux, macOS for Apple targets)
- **Depends on**: `check-version`, `create-release`
- **Strategy**: Matrix build for multiple platforms
- **Platforms**:
  - `x86_64-unknown-linux-gnu` (Ubuntu)
  - `aarch64-apple-darwin` (macOS)
  - `x86_64-apple-darwin` (macOS)
- **Process**:
  1. Sets up Rust toolchain with target
  2. Installs Diesel CLI for database setup
  3. Builds release binary for target platform
  4. Creates compressed archive
  5. Uploads to GitHub release

### Job 4: `build-universal-macos`
- **Runs on**: `macos-latest`
- **Depends on**: `check-version`, `create-release`
- **Purpose**: Create universal macOS binary combining x86_64 and ARM64
- **Process**:
  1. Builds for both macOS architectures
  2. Uses `lipo` to create universal binary
  3. Creates archive and uploads to release

## Platform Support

| Platform | Architecture | Archive Name | Runner |
|----------|-------------|--------------|---------|
| Linux | x86_64 | `v{version}-x86_64-unknown-linux-gnu.tar.gz` | ubuntu-latest |
| macOS | x86_64 | `v{version}-x86_64-apple-darwin.tar.gz` | macos-latest |
| macOS | ARM64 | `v{version}-aarch64-apple-darwin.tar.gz` | macos-latest |
| macOS | Universal | `v{version}-universal-apple-darwin.tar.gz` | macos-latest |

## Version Detection Logic

The workflow uses a simple but effective version detection mechanism:

1. **Current Version**: Extracted from `Cargo.toml` using `grep` and `sed`
2. **Previous Version**: Extracted from `Cargo.toml` in the previous commit (`HEAD~1`)
3. **Comparison**: String comparison to detect changes
4. **Validation**: Ensures version follows semantic versioning format

## Usage for Developers

### Triggering a Release

1. Update version in `Cargo.toml`:
   ```toml
   [workspace.package]
   version = "0.3.1"  # Changed from "0.3.0"
   ```

2. Commit and push to main:
   ```bash
   git add Cargo.toml
   git commit -m "bump version to 0.3.1"
   git push origin main
   ```

3. GitHub Actions will automatically:
   - Detect the version change
   - Build all platform binaries
   - Create release `v0.3.1`
   - Upload all assets

### Local Testing

Before triggering the automated release, test locally:

```bash
# Verify version format
make check-version

# Test local builds (macOS only)
make release-local
```

## Troubleshooting

### Common Issues

1. **Version Detection Fails**
   - **Cause**: Version format doesn't match expected pattern
   - **Solution**: Ensure version follows `X.Y.Z` format in `Cargo.toml`

2. **Build Failures**
   - **Cause**: Missing dependencies or compilation errors
   - **Solution**: Ensure code builds locally first with `make ci`

3. **Archive Creation Issues**
   - **Cause**: Binary not found or permissions issues
   - **Solution**: Check that binary name matches `cli` as specified

4. **Upload Failures**
   - **Cause**: GitHub token permissions or API issues
   - **Solution**: Check repository settings and GitHub token permissions

### Debugging Steps

1. **Check Workflow Logs**: Visit GitHub Actions tab to view detailed logs
2. **Verify Version**: Ensure version changed correctly in `Cargo.toml`
3. **Test Locally**: Run `make release-local` to test build process
4. **Check Dependencies**: Ensure all required tools are available

## Security Considerations

### Token Usage
- **GITHUB_TOKEN**: Automatically provided by GitHub Actions
- **Permissions**: Workflow has `contents: write` permission for creating releases
- **Scope**: Limited to repository operations only

### Binary Security
- All binaries are built in isolated GitHub Actions runners
- No external dependencies beyond standard Rust toolchain
- Reproducible builds using locked dependencies (`Cargo.lock`)

## Manual Fallback

If the automated workflow fails, you can still create releases manually:

1. **Tag the Release**:
   ```bash
   git tag v0.3.1
   git push origin v0.3.1
   ```

2. **Existing Workflow**: The existing `release.yaml` workflow will trigger on the tag

3. **Manual Process**: Build and upload binaries manually if needed

## Future Enhancements

### Potential Improvements

1. **Changelog Generation**: Automatically generate changelogs from commit messages
2. **Binary Signing**: Sign binaries for additional security
3. **Checksums**: Include SHA256 checksums for all assets
4. **Pre-release Support**: Handle pre-release versions (alpha, beta, rc)
5. **Notification Integration**: Notify team via Slack/Discord on successful releases
6. **Caching**: Cache Rust dependencies to speed up builds

### Monitoring

- **Success Rate**: Monitor workflow success/failure rates
- **Build Times**: Track build duration for optimization
- **Download Metrics**: Monitor release download statistics
- **Error Patterns**: Track common failure modes for improvement

## Conclusion

The automated release workflow provides:

- **Consistency**: Every release follows the same process
- **Reliability**: Reduces human error in release creation
- **Efficiency**: Eliminates manual steps and waiting time
- **Traceability**: Complete audit trail of all releases
- **Multi-platform Support**: Automatic builds for all supported platforms

This system ensures that Trust releases are professional, reliable, and accessible to users across all supported platforms.