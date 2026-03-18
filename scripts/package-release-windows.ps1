param(
    [Parameter(Mandatory=$true)]
    [string]$Label,
    [switch]$Debug
)

$ErrorActionPreference = "Stop"
$AppName = "diske"
$RepoDir = (Resolve-Path "$PSScriptRoot/..").Path
Set-Location $RepoDir

$Profile = if ($Debug) { "debug" } else { "release" }
$Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value

$DistDir = Join-Path $RepoDir "dist"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

# Build
if ($Profile -eq "release") {
    cargo build --release
} else {
    cargo build
}

$BinaryPath = Join-Path $RepoDir "target/$Profile/$AppName.exe"
if (-not (Test-Path $BinaryPath)) {
    Write-Error "Expected binary not found: $BinaryPath"
    exit 1
}

# Package as zip
$ZipName = "$AppName-v$Version-$Label.zip"
$ZipPath = Join-Path $DistDir $ZipName
if (Test-Path $ZipPath) { Remove-Item $ZipPath }
Compress-Archive -Path $BinaryPath -DestinationPath $ZipPath

# Build info
$BuildInfo = Join-Path $DistDir "$AppName-v$Version-$Label.build-info.txt"
@"
app=$AppName
version=$Version
profile=$Profile
built_at_utc=$(Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ" -AsUTC)
"@ | Set-Content $BuildInfo

# Checksums
$Checksums = Join-Path $DistDir "SHA256SUMS-$Label.txt"
$items = @($ZipPath, $BuildInfo)
$items | ForEach-Object {
    $hash = (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower()
    "$hash  $_"
} | Set-Content $Checksums

Write-Host "Packaged assets:"
Write-Host " - $ZipPath"
Write-Host " - $BuildInfo"
Write-Host " - $Checksums"
