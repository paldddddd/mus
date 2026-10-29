<#
.SYNOPSIS
    Automated production build and packaging script for wallmus.
.DESCRIPTION
    1. Compiles frontend assets (Vite + TypeScript).
    2. Builds release binaries for daemon, UI, and HTML host.
    3. Assembles distribution directory with all LGPL dependencies (libmpv, ffmpeg).
    4. Generates a standalone Portable ZIP release with portable.dat marker.
    5. Compiles Inno Setup installer (if ISCC.exe is available).
    6. Signs binaries (if sign credentials are provided).
#>

param(
    [switch]$SkipFrontend = $false,
    [switch]$Sign = $false
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Resolve-Path "$PSScriptRoot\.."
$DistDir = Join-Path $ProjectRoot "dist"
$Version = "0.2.0"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  wallmus Production Build & Packaging (v$Version)        " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Clean and prepare dist directory
if (Test-Path $DistDir) {
    Remove-Item $DistDir -Recurse -Force
}
New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
$PackageDir = Join-Path $DistDir "wallmus"
New-Item -ItemType Directory -Path $PackageDir -Force | Out-Null

# 2. Build Frontend
if (-not $SkipFrontend) {
    Write-Host "`n[1/5] Building frontend assets (TypeScript + Vite)..." -ForegroundColor Yellow
    Push-Location (Join-Path $ProjectRoot "ui")
    try {
        cmd /c "npm run build"
        if ($LASTEXITCODE -ne 0) {
            throw "Frontend build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[1/5] Skipping frontend build per parameter..." -ForegroundColor DarkGray
}

# 3. Build Rust Binaries in Release Mode
Write-Host "`n[2/5] Compiling Rust binaries in --release mode..." -ForegroundColor Yellow
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release -p wallmus -p wallmus-ui -p html-host
if ($LASTEXITCODE -ne 0) {
    throw "Cargo release build failed with exit code $LASTEXITCODE"
}

# 4. Assemble package files
Write-Host "`n[3/5] Assembling distribution files and third-party dependencies..." -ForegroundColor Yellow
$TargetRelease = Join-Path $ProjectRoot "target\release"
$DepsDir = Join-Path $ProjectRoot "deps"

# Copy binaries
Copy-Item (Join-Path $TargetRelease "wallmus.exe") $PackageDir -Force
Copy-Item (Join-Path $TargetRelease "wallmus-ui.exe") $PackageDir -Force
Copy-Item (Join-Path $TargetRelease "wallmus-htmlhost.exe") $PackageDir -Force

# Copy external libraries
Copy-Item (Join-Path $DepsDir "libmpv-2.dll") $PackageDir -Force
Copy-Item (Join-Path $DepsDir "ffmpeg.exe") $PackageDir -Force
Copy-Item (Join-Path $DepsDir "ffprobe.exe") $PackageDir -Force

# Copy license and readme
Copy-Item (Join-Path $ProjectRoot "LICENSE.txt") $PackageDir -Force

# Copy bundled assets (default wallpapers)
$AssetsDir = Join-Path $ProjectRoot "assets"
if (Test-Path $AssetsDir) {
    Copy-Item $AssetsDir $PackageDir -Recurse -Force
}

Write-Host "Package assembled in: $PackageDir" -ForegroundColor Green

# 5. Create Portable Package
Write-Host "`n[4/5] Creating portable release (with portable.dat marker)..." -ForegroundColor Yellow
$PortableDir = Join-Path $DistDir "wallmus_portable"
Copy-Item $PackageDir $PortableDir -Recurse -Force

# Create portable.dat marker file
New-Item -ItemType File -Path (Join-Path $PortableDir "portable.dat") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $PortableDir "data") -Force | Out-Null

$PortableZip = Join-Path $DistDir "wallmus-v$Version-portable.zip"
if (Test-Path $PortableZip) { Remove-Item $PortableZip -Force }
Compress-Archive -Path "$PortableDir\*" -DestinationPath $PortableZip -CompressionLevel Optimal
Remove-Item $PortableDir -Recurse -Force
Write-Host "Portable package created: $PortableZip" -ForegroundColor Green

# 6. Optional Code Signing
if ($Sign) {
    Write-Host "`n[*] Signing binaries..." -ForegroundColor Yellow
    & "$PSScriptRoot\sign.ps1" -TargetPath $PackageDir
}

# 7. Compile Inno Setup Installer
Write-Host "`n[5/5] Checking for Inno Setup compiler (ISCC.exe)..." -ForegroundColor Yellow
$IsccPath = Get-Command iscc.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1

if (-not $IsccPath) {
    $CommonPaths = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )
    foreach ($p in $CommonPaths) {
        if (Test-Path $p) {
            $IsccPath = $p
            break
        }
    }
}

if ($IsccPath) {
    Write-Host "Compiling Inno Setup installer via: $IsccPath" -ForegroundColor Cyan
    $IssScript = Join-Path $ProjectRoot "installer\installer.iss"
    & $IsccPath $IssScript
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Installer compiled successfully in: $DistDir" -ForegroundColor Green
    } else {
        Write-Warning "Inno Setup compiler exited with error code $LASTEXITCODE"
    }
} else {
    Write-Host "ISCC.exe not found on system. To build installer manually:" -ForegroundColor Gray
    Write-Host "  1. Install Inno Setup 6 (https://jrsoftware.org/isdl.php)" -ForegroundColor Gray
    Write-Host "  2. Run: iscc.exe installer\installer.iss" -ForegroundColor Gray
}

Write-Host "`n==========================================================" -ForegroundColor Green
Write-Host "  Build and packaging completed successfully!             " -ForegroundColor Green
Write-Host "  Output artifacts available in: dist\                   " -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
