# Win-CodexBar CI/CD

Win-CodexBar release builds use CircleCI hosted Windows first. Buildkite is kept
ready for a future non-AWS self-hosted Windows runner. Cloudflare R2 mirrors the
release artifacts after the Windows smoke test passes.

## Current Model

- Primary release CI: CircleCI hosted Windows.
- Artifact mirror: Cloudflare R2.
- Future self-hosted CI: Buildkite queue `windows-release`.
- Not used for automated release builds: GitHub Actions, AWS EC2, or an always-on
  FSOS runner.

The canonical Windows build path remains:

```powershell
powershell.exe -File scripts\release-doctor.ps1 -SkipGitHub
powershell.exe -File scripts\windows-release-build.ps1 -Ref v0.30.0 -SmokeInstall
```

CI wrappers must call those scripts instead of duplicating the release logic.

## CircleCI

CircleCI builds tagged releases with the Windows executor from `.circleci/config.yml`.
The tag workflow runs for tags matching `v*`.

Manual release checks can be started from CircleCI with pipeline parameters:

- `run_windows_release=true`
- `upload_cloudflare=false` for dry/manual validation
- `upload_cloudflare=true` only when Cloudflare R2 secrets are configured

Required CircleCI project environment variables for Cloudflare upload:

- `CLOUDFLARE_ACCOUNT_ID`
- `CLOUDFLARE_R2_BUCKET`
- `CLOUDFLARE_R2_ACCESS_KEY_ID`
- `CLOUDFLARE_R2_SECRET_ACCESS_KEY`

`GITHUB_TOKEN` is not required by default. CircleCI should not upload to GitHub
Releases unless that behavior is explicitly added and reviewed later.

## Cloudflare R2

Cloudflare R2 is a mirror/proof layer, not the source of truth for publishing.
The upload script uses R2's S3-compatible API directly:

```powershell
powershell.exe -File scripts\ci\upload-cloudflare-r2.ps1 -Version 0.30.0 -DryRun
powershell.exe -File scripts\ci\upload-cloudflare-r2.ps1 -Version 0.30.0
```

Object layout:

```text
releases/v0.30.0/CodexBar-0.30.0-Setup.exe
releases/v0.30.0/CodexBar-0.30.0-Setup.exe.sha256
releases/v0.30.0/CodexBar-0.30.0-portable.exe
releases/v0.30.0/CodexBar-0.30.0-portable.exe.sha256
releases/v0.30.0/release-manifest.json
releases/v0.30.0/smoke-test-log.txt
```

`smoke-test-log.txt` is uploaded only when the Windows smoke test produced it.

## Buildkite

`.buildkite/pipeline.yml` is a future-runner skeleton. It requires a Windows
agent attached to the `windows-release` queue and does not start, stop, or manage
AWS instances.

The Buildkite release path requires `BUILDKITE_TAG` and runs:

```powershell
powershell.exe -File scripts\ci\buildkite-release.ps1
```

Do not make Buildkite mandatory until a non-AWS Windows host is attached.

## Release Flow

1. Tag the release, for example `v0.30.0`.
2. CircleCI hosted Windows builds the installer and portable exe.
3. `scripts\windows-smoke-install.ps1` installs, verifies, and uninstalls the app.
4. CircleCI stores release assets and SHA-256 sidecars.
5. Cloudflare R2 mirrors the assets when secrets are configured.
6. Publish or update the GitHub release with the verified assets.
7. Submit the Winget manifest update after the GitHub installer URL is stable.
