#Requires -Version 5.1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
trap {
    Write-Error $_
    exit 1
}

if (-not $env:BUILDKITE_TAG) {
    throw "Buildkite release builds require BUILDKITE_TAG, for example v0.30.0."
}

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$WorkRoot = "C:\code\Win-CodexBar-release"

Push-Location $RepoRoot
try {
    $preBuildAssetsDir = Join-Path $env:TEMP ("win-codexbar-no-prebuild-assets-" + [guid]::NewGuid().ToString("n"))
    & powershell.exe -NoLogo -ExecutionPolicy Bypass -File "scripts\release-doctor.ps1" -SkipGitHub -AssetsDir $preBuildAssetsDir
    if ($LASTEXITCODE -ne 0) {
        throw "release-doctor.ps1 failed with exit code $LASTEXITCODE"
    }

    & powershell.exe -NoLogo -ExecutionPolicy Bypass -File "scripts\windows-release-build.ps1" -Ref $env:BUILDKITE_TAG -WorkRoot $WorkRoot -SmokeInstall
    if ($LASTEXITCODE -ne 0) {
        throw "windows-release-build.ps1 failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
