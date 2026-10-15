param(
    [ValidateSet("arm64-v8a", "x86_64")]
    [string]$Target = "arm64-v8a",
    [ValidateSet("debug", "release")]
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

$sdkDir = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA/Android/Sdk" }
$adb = "$sdkDir/platform-tools/adb.exe"

if (-not (Test-Path $adb)) {
    throw "adb not found at $adb"
}

$apkPath = "target/groshe-$Profile-$Target.apk"
if (-not (Test-Path $apkPath)) {
    throw "APK not found at $apkPath. Run package-apk.ps1 first."
}

Write-Host "Checking for connected Android devices via adb..." -ForegroundColor Cyan
$devices = & $adb devices | Select-String -Pattern "\b(device|emulator)\b"

if (-not $devices) {
    Write-Warning "No connected Android devices or emulators found via adb."
    Write-Host "`nTo connect a physical Android reference phone:" -ForegroundColor Yellow
    Write-Host "  1. Connect your phone via USB cable (or enable Wireless Debugging)."
    Write-Host "  2. On phone: Go to Settings -> Developer Options -> Enable 'USB Debugging'."
    Write-Host "  3. Accept the 'Allow USB debugging from this computer' RSA prompt on phone."
    Write-Host "  4. For wireless debugging: & `"$adb`" connect <phone-ip>:<port>"
    Write-Host "  5. Re-run: just android-run Target=$Target Profile=$Profile"
    exit 0
}

Write-Host "Device detected: $($devices[0])" -ForegroundColor Green
Write-Host "Installing $apkPath onto device..." -ForegroundColor Cyan
& $adb install -r -d $apkPath
if ($LASTEXITCODE -ne 0) {
    throw "adb install failed"
}

Write-Host "Stopping any running instance..." -ForegroundColor Cyan
& $adb shell am force-stop com.groshe.app

Write-Host "`nMeasuring Cold Start Time via ActivityManager..." -ForegroundColor Cyan
$startOutput = & $adb shell am start -W -n com.groshe.app/android.app.NativeActivity
$startOutput | ForEach-Object { Write-Host "  $_" }

$totalTimeLine = $startOutput | Select-String "TotalTime:\s*(\d+)"
if ($totalTimeLine -match "TotalTime:\s*(\d+)") {
    $totalMs = [int]$matches[1]
    $sec = [math]::Round($totalMs / 1000.0, 3)
    Write-Host "`nObserved Cold Start Time: $totalMs ms ($sec s)" -ForegroundColor Green
    if ($totalMs -le 1500) {
        Write-Host "Exit Criteria Passed: $totalMs ms <= 1500 ms target." -ForegroundColor Green
    } else {
        Write-Warning "Cold start time exceeds 1500 ms target: $totalMs ms."
    }
}
