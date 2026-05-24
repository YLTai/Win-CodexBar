#Requires -Version 5.1
<#
.SYNOPSIS
    Upload Win-CodexBar release artifacts to Cloudflare R2.

.DESCRIPTION
    Uses Cloudflare R2's S3-compatible API directly from PowerShell. This keeps
    the release mirror independent from AWS infrastructure and avoids requiring
    an extra CLI on the Windows builder.
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$AssetsDir = "C:\code\Win-CodexBar-release\assets",

    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RequiredEnv {
    param([string]$Name)

    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "Missing required environment variable: $Name"
    }
    return $value
}

function Get-Sha256Hex {
    param([byte[]]$Bytes)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return -join (($sha.ComputeHash($Bytes)) | ForEach-Object { $_.ToString("x2") })
    } finally {
        $sha.Dispose()
    }
}

function Get-HmacSha256 {
    param(
        [byte[]]$Key,
        [string]$Data
    )

    $hmac = [System.Security.Cryptography.HMACSHA256]::new($Key)
    try {
        return $hmac.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Data))
    } finally {
        $hmac.Dispose()
    }
}

function Get-SignatureKey {
    param(
        [string]$SecretKey,
        [string]$DateStamp,
        [string]$Region,
        [string]$Service
    )

    $kSecret = [System.Text.Encoding]::UTF8.GetBytes("AWS4$SecretKey")
    $kDate = Get-HmacSha256 -Key $kSecret -Data $DateStamp
    $kRegion = Get-HmacSha256 -Key $kDate -Data $Region
    $kService = Get-HmacSha256 -Key $kRegion -Data $Service
    return Get-HmacSha256 -Key $kService -Data "aws4_request"
}

function ConvertTo-Hex {
    param([byte[]]$Bytes)
    return -join ($Bytes | ForEach-Object { $_.ToString("x2") })
}

function ConvertTo-S3Path {
    param([string]$Path)

    return (($Path -split "/") | ForEach-Object { [Uri]::EscapeDataString($_) }) -join "/"
}

function Send-R2Object {
    param(
        [string]$FilePath,
        [string]$ObjectKey,
        [string]$ContentType,
        [string]$AccountId,
        [string]$Bucket,
        [string]$AccessKeyId,
        [string]$SecretAccessKey
    )

    $hostName = "$AccountId.r2.cloudflarestorage.com"
    $region = "auto"
    $service = "s3"
    $now = (Get-Date).ToUniversalTime()
    $amzDate = $now.ToString("yyyyMMddTHHmmssZ")
    $dateStamp = $now.ToString("yyyyMMdd")
    $fileBytes = [IO.File]::ReadAllBytes($FilePath)
    $payloadHash = Get-Sha256Hex -Bytes $fileBytes
    $canonicalUri = "/" + (ConvertTo-S3Path "$Bucket/$ObjectKey")
    $signedHeaders = "host;x-amz-content-sha256;x-amz-date"
    $canonicalHeaders = "host:$hostName`nx-amz-content-sha256:$payloadHash`nx-amz-date:$amzDate`n"
    $canonicalRequest = "PUT`n$canonicalUri`n`n$canonicalHeaders`n$signedHeaders`n$payloadHash"
    $credentialScope = "$dateStamp/$region/$service/aws4_request"
    $canonicalRequestHash = Get-Sha256Hex -Bytes ([Text.Encoding]::UTF8.GetBytes($canonicalRequest))
    $stringToSign = "AWS4-HMAC-SHA256`n$amzDate`n$credentialScope`n$canonicalRequestHash"
    $signingKey = Get-SignatureKey -SecretKey $SecretAccessKey -DateStamp $dateStamp -Region $region -Service $service
    $signature = ConvertTo-Hex (Get-HmacSha256 -Key $signingKey -Data $stringToSign)
    $authorization = "AWS4-HMAC-SHA256 Credential=$AccessKeyId/$credentialScope, SignedHeaders=$signedHeaders, Signature=$signature"
    $uri = "https://$hostName$canonicalUri"

    $headers = @{
        "Authorization" = $authorization
        "x-amz-content-sha256" = $payloadHash
        "x-amz-date" = $amzDate
    }

    Write-Host "Uploading $FilePath -> r2://$Bucket/$ObjectKey"
    Invoke-WebRequest -Method Put -Uri $uri -Headers $headers -ContentType $ContentType -InFile $FilePath | Out-Null
}

function Get-ContentType {
    param([string]$Path)

    switch ([IO.Path]::GetExtension($Path).ToLowerInvariant()) {
        ".json" { "application/json" }
        ".sha256" { "text/plain" }
        ".txt" { "text/plain" }
        default { "application/octet-stream" }
    }
}

if (-not (Test-Path $AssetsDir)) {
    throw "Assets directory does not exist: $AssetsDir"
}

$tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
$plainVersion = $Version.TrimStart("v")
$releasePrefix = "releases/$tag"
$assetNames = @(
    "CodexBar-$plainVersion-Setup.exe",
    "CodexBar-$plainVersion-Setup.exe.sha256",
    "CodexBar-$plainVersion-portable.exe",
    "CodexBar-$plainVersion-portable.exe.sha256"
)

$uploads = New-Object System.Collections.Generic.List[object]
foreach ($name in $assetNames) {
    $path = Join-Path $AssetsDir $name
    if (-not (Test-Path $path)) {
        throw "Missing release asset: $path"
    }
    $uploads.Add([pscustomobject]@{
        Path = $path
        Key = "$releasePrefix/$name"
        Name = $name
        Sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        Size = (Get-Item -LiteralPath $path).Length
    })
}

$smokeLog = Join-Path $AssetsDir "smoke-test-log.txt"
if (Test-Path $smokeLog) {
    $uploads.Add([pscustomobject]@{
        Path = $smokeLog
        Key = "$releasePrefix/smoke-test-log.txt"
        Name = "smoke-test-log.txt"
        Sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $smokeLog).Hash.ToLowerInvariant()
        Size = (Get-Item -LiteralPath $smokeLog).Length
    })
}

$manifestPath = Join-Path $AssetsDir "release-manifest.json"
$manifest = [pscustomobject]@{
    version = $plainVersion
    tag = $tag
    built_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    assets = @($uploads | ForEach-Object {
        [pscustomobject]@{
            name = $_.Name
            key = $_.Key
            sha256 = $_.Sha256
            size = $_.Size
        }
    })
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $manifestPath
$uploads.Add([pscustomobject]@{
    Path = $manifestPath
    Key = "$releasePrefix/release-manifest.json"
    Name = "release-manifest.json"
    Sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
    Size = (Get-Item -LiteralPath $manifestPath).Length
})

if ($DryRun) {
    Write-Host "Cloudflare R2 dry run:"
    $uploads | Select-Object Name, Key, Size, Sha256 | Format-Table -AutoSize
    return
}

$accountId = Get-RequiredEnv "CLOUDFLARE_ACCOUNT_ID"
$bucket = Get-RequiredEnv "CLOUDFLARE_R2_BUCKET"
$accessKeyId = Get-RequiredEnv "CLOUDFLARE_R2_ACCESS_KEY_ID"
$secretAccessKey = Get-RequiredEnv "CLOUDFLARE_R2_SECRET_ACCESS_KEY"

foreach ($upload in $uploads) {
    Send-R2Object `
        -FilePath $upload.Path `
        -ObjectKey $upload.Key `
        -ContentType (Get-ContentType $upload.Path) `
        -AccountId $accountId `
        -Bucket $bucket `
        -AccessKeyId $accessKeyId `
        -SecretAccessKey $secretAccessKey
}

Write-Host "Cloudflare R2 upload complete: r2://$bucket/$releasePrefix/"
