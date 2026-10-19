<#
.SYNOPSIS
    Code signing script for wallmus binaries and installer.
.DESCRIPTION
    Signs executables using Windows SDK signtool.exe with an Authenticode certificate.
.PARAMETER TargetPath
    Path to file or directory containing files to sign. Defaults to "dist".
.PARAMETER PfxPath
    Path to .pfx certificate. Can also be set via $env:CODE_SIGN_PFX.
.PARAMETER Password
    Password for the certificate. Can also be set via $env:CODE_SIGN_PASSWORD.
#>

param(
    [string]$TargetPath = "dist",
    [string]$PfxPath = $env:CODE_SIGN_PFX,
    [string]$Password = $env:CODE_SIGN_PASSWORD
)

$ErrorActionPreference = "Stop"

Write-Host "=== wallmus Code Signing Utility ===" -ForegroundColor Cyan

# 1. Locate signtool.exe
$SigntoolPath = Get-Command signtool.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1

if (-not $SigntoolPath) {
    $SdkCandidates = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending
    if ($SdkCandidates) {
        $SigntoolPath = $SdkCandidates[0].FullName
    }
}

if (-not $SigntoolPath) {
    Write-Warning "signtool.exe was not found on this system (Windows 10/11 SDK required)."
    Write-Host "Code signing skipped. Binaries remain unsigned for local development." -ForegroundColor Yellow
    exit 0
}

Write-Host "Found signtool: $SigntoolPath" -ForegroundColor Green

# 2. Check Certificate
if (-not $PfxPath -or -not (Test-Path $PfxPath)) {
    Write-Warning "No valid code signing PFX certificate provided (-PfxPath or `$env:CODE_SIGN_PFX)."
    Write-Host "To sign binaries in production or CI/CD, set:" -ForegroundColor Yellow
    Write-Host '  $env:CODE_SIGN_PFX = "C:\path\to\cert.pfx"' -ForegroundColor Gray
    Write-Host '  $env:CODE_SIGN_PASSWORD = "your_password"' -ForegroundColor Gray
    Write-Host "Skipping signature generation." -ForegroundColor Yellow
    exit 0
}

# 3. Collect files to sign
$FilesToSign = @()
if (Test-Path $TargetPath -PathType Container) {
    $FilesToSign = Get-ChildItem $TargetPath -Include *.exe, *.dll -Recurse | Select-Object -ExpandProperty FullName
} elseif (Test-Path $TargetPath -PathType Leaf) {
    $FilesToSign = @($TargetPath)
}

if ($FilesToSign.Count -eq 0) {
    Write-Warning "No files found to sign in $TargetPath"
    exit 0
}

$TimestampServer = "http://timestamp.digicert.com"

foreach ($file in $FilesToSign) {
    Write-Host "Signing: $file ..." -ForegroundColor Cyan
    $ArgsList = @(
        "sign",
        "/fd", "SHA256",
        "/f", $PfxPath
    )
    if ($Password) {
        $ArgsList += @("/p", $Password)
    }
    $ArgsList += @(
        "/tr", $TimestampServer,
        "/td", "SHA256",
        $file
    )

    $proc = Start-Process -FilePath $SigntoolPath -ArgumentList $ArgsList -Wait -NoNewWindow -PassThru
    if ($proc.ExitCode -eq 0) {
        Write-Host "Successfully signed: $file" -ForegroundColor Green
    } else {
        Write-Error "Failed to sign $file (ExitCode: $($proc.ExitCode))"
    }
}

Write-Host "=== Code signing process completed ===" -ForegroundColor Green
