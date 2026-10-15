<#
.SYNOPSIS
    Groshe Windows Development Environment Bootstrapper
.DESCRIPTION
    Validates and configures Rust MSRV, Android targets, cargo-ndk, OpenJDK 17,
    Android SDK, and Android NDK on Windows.
#>

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

function Write-Step($msg) {
    Write-Host "`n==> $msg" -ForegroundColor Cyan
}

function Write-Success($msg) {
    Write-Host " [OK] $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
    Write-Host " [WARN] $msg" -ForegroundColor Yellow
}

function Write-Info($msg) {
    Write-Host " [INFO] $msg" -ForegroundColor Gray
}

Write-Host "==========================================================" -ForegroundColor Blue
Write-Host " Groshe Windows Development Environment Bootstrapper " -ForegroundColor Blue
Write-Host "==========================================================" -ForegroundColor Blue

# 1. Rust and MSRV Check
Write-Step "Checking Rust toolchain and MSRV..."
$msrv = [version]"1.80.0"

if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    throw "rustc is not installed. Please install rustup from https://rustup.rs"
}

$rustcVersionRaw = (rustc --version).Split(" ")[1]
$rustcVersion = [version]($rustcVersionRaw.Split("-")[0])

if ($rustcVersion -lt $msrv) {
    throw "Installed rustc version ($rustcVersion) is below MSRV ($msrv). Please run 'rustup update stable'."
}
Write-Success "Rust version $rustcVersion installed (MSRV >= $msrv satisfied)"

# Check and add Android Rust targets
$installedTargets = rustup target list --installed
foreach ($target in @("aarch64-linux-android", "x86_64-linux-android")) {
    if ($installedTargets -contains $target) {
        Write-Success "Rust target $target is installed"
    } else {
        Write-Info "Installing missing Rust target $target..."
        rustup target add $target
        Write-Success "Target $target installed successfully"
    }
}

# Check clippy and rustfmt
$components = rustup component list --installed
foreach ($comp in @("clippy", "rustfmt")) {
    if ($components -match "^$comp") {
        Write-Success "Rust component $comp is installed"
    } else {
        Write-Info "Installing missing component $comp..."
        rustup component add $comp
    }
}

# 2. Check cargo-ndk
Write-Step "Checking cargo-ndk utility..."
if (Get-Command cargo-ndk -ErrorAction SilentlyContinue) {
    $ndkVersion = (cargo ndk --version)
    Write-Success "$ndkVersion installed"
} else {
    Write-Info "cargo-ndk not found. Installing via 'cargo install cargo-ndk'..."
    cargo install cargo-ndk
    Write-Success "cargo-ndk installed successfully"
}

# 3. Check Java (JDK 17+)
Write-Step "Checking Java (JDK 17+)..."
$jdkDir = "$env:USERPROFILE\.jdks\jdk-17.0.20.1+1"
$javaCmd = Get-Command java -ErrorAction SilentlyContinue

$hasValidJava = $false
if ($javaCmd) {
    $javaVerStr = (java -version 2>&1 | Out-String)
    if ($javaVerStr -match 'version "(17|21|[2-9][0-9])') {
        $hasValidJava = $true
        Write-Success "System Java runtime detected"
    }
}

if (-not $hasValidJava) {
    if (Test-Path "$jdkDir\bin\javac.exe") {
        Write-Success "Found portable JDK at $jdkDir"
    } else {
        Write-Info "Downloading portable OpenJDK 17 from Adoptium..."
        New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.jdks" | Out-Null
        $zipPath = "$env:TEMP\jdk17.zip"
        $downloadUrl = "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.20.1%2B1/OpenJDK17U-jdk_x64_windows_hotspot_17.0.20.1_1.zip"
        Start-BitsTransfer -Source $downloadUrl -Destination $zipPath
        Expand-Archive -Path $zipPath -DestinationPath "$env:USERPROFILE\.jdks" -Force
        Remove-Item $zipPath -Force
        Write-Success "Portable OpenJDK 17 installed to $jdkDir"
    }

    # Configure User Environment
    [Environment]::SetEnvironmentVariable("JAVA_HOME", $jdkDir, "User")
    $env:JAVA_HOME = $jdkDir
    $env:Path = "$jdkDir\bin;$env:Path"
}

# 4. Check Android SDK
Write-Step "Checking Android SDK..."
$sdkDir = if ($env:ANDROID_HOME -and (Test-Path $env:ANDROID_HOME)) {
    $env:ANDROID_HOME
} else {
    "$env:LOCALAPPDATA\Android\Sdk"
}

if (-not (Test-Path $sdkDir)) {
    throw "Android SDK not found at $sdkDir. Please install the Android SDK or set ANDROID_HOME."
}

[Environment]::SetEnvironmentVariable("ANDROID_HOME", $sdkDir, "User")
[Environment]::SetEnvironmentVariable("ANDROID_SDK_ROOT", $sdkDir, "User")
$env:ANDROID_HOME = $sdkDir
$env:ANDROID_SDK_ROOT = $sdkDir
Write-Success "Android SDK located at $sdkDir"

# Check android-34 platform
$platform34 = "$sdkDir\platforms\android-34\android.jar"
if (Test-Path $platform34) {
    Write-Success "Android platform API 34 (android.jar) verified"
} else {
    Write-Warn "android-34 platform not found at $platform34. Android builds will require platform android-34."
}

# 5. Check Android NDK
Write-Step "Checking Android NDK..."
$ndkBase = "$sdkDir\ndk"
$ndkDir = if ($env:ANDROID_NDK_HOME -and (Test-Path $env:ANDROID_NDK_HOME)) {
    $env:ANDROID_NDK_HOME
} elseif (Test-Path "$ndkBase\android-ndk-r26b") {
    "$ndkBase\android-ndk-r26b"
} else {
    $existing = Get-ChildItem $ndkBase -Directory -ErrorAction SilentlyContinue | Select-Object -Last 1
    if ($existing) { $existing.FullName } else { $null }
}

if (-not $ndkDir -or -not (Test-Path "$ndkDir\source.properties")) {
    Write-Info "NDK not found. Downloading Google Android NDK r26b (~630MB)..."
    New-Item -ItemType Directory -Force -Path $ndkBase | Out-Null
    $zipPath = "$env:TEMP\ndk-r26b.zip"
    curl.exe -L -o $zipPath "https://dl.google.com/android/repository/android-ndk-r26b-windows.zip"
    Write-Info "Extracting NDK..."
    tar.exe -xf $zipPath -C $ndkBase
    Remove-Item $zipPath -Force
    $ndkDir = "$ndkBase\android-ndk-r26b"
    Write-Success "Android NDK r26b installed to $ndkDir"
} else {
    Write-Success "Android NDK verified at $ndkDir"
}

[Environment]::SetEnvironmentVariable("ANDROID_NDK_HOME", $ndkDir, "User")
[Environment]::SetEnvironmentVariable("ANDROID_NDK_ROOT", $ndkDir, "User")
$env:ANDROID_NDK_HOME = $ndkDir
$env:ANDROID_NDK_ROOT = $ndkDir

Write-Host "`n==========================================================" -ForegroundColor Green
Write-Host " Environment validation and bootstrap complete!" -ForegroundColor Green
Write-Host " All prerequisites for mobile and desktop are ready." -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
