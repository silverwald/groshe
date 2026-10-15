param(
    [ValidateSet("arm64-v8a", "x86_64")]
    [string]$Target = "arm64-v8a",
    [ValidateSet("debug", "release")]
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

$rustTarget = if ($Target -eq "arm64-v8a") { "aarch64-linux-android" } else { "x86_64-linux-android" }

# Resolve SDK, NDK, and Java paths
$sdkDir = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA/Android/Sdk" }
$ndkDir = if ($env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME } else { "$sdkDir/ndk/android-ndk-r26b" }
$jdkDir = if ($env:JAVA_HOME) { $env:JAVA_HOME } else { "$env:USERPROFILE/.jdks/jdk-17.0.20.1+1" }

$aapt2 = "$sdkDir/build-tools/34.0.0/aapt2.exe"
$zipalign = "$sdkDir/build-tools/34.0.0/zipalign.exe"
$apksigner = "$sdkDir/build-tools/34.0.0/apksigner.bat"
$androidJar = "$sdkDir/platforms/android-34/android.jar"
$llvmStrip = "$ndkDir/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-strip.exe"
$keytool = "$jdkDir/bin/keytool.exe"
$keystore = "$env:USERPROFILE/.android/debug.keystore"
$manifest = "apps/android/android/AndroidManifest.xml"

Write-Host "=== Groshe Android APK Packager ===" -ForegroundColor Cyan
Write-Host "Target:  $Target ($rustTarget)"
Write-Host "Profile: $Profile"

# 1. Ensure debug keystore exists
if (-not (Test-Path $keystore)) {
    Write-Host "Generating debug keystore at $keystore..." -ForegroundColor Yellow
    $androidDir = Split-Path $keystore -Parent
    if (-not (Test-Path $androidDir)) { New-Item -ItemType Directory -Path $androidDir -Force | Out-Null }
    & $keytool -genkeypair -v -keystore $keystore -storepass android -alias androiddebugkey -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
}

# 2. Build native shared library if missing
$soPath = "target/$rustTarget/$Profile/libgroshe_android.so"
if (-not (Test-Path $soPath)) {
    Write-Host "Building $soPath via cargo-ndk..." -ForegroundColor Yellow
    $buildArgs = @("ndk", "-t", $Target, "build", "-p", "groshe-android")
    if ($Profile -eq "release") { $buildArgs += "--release" }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo-ndk build failed" }
}

# 3. Strip release binary
if ($Profile -eq "release" -and (Test-Path $llvmStrip)) {
    Write-Host "Stripping symbols with llvm-strip..." -ForegroundColor Yellow
    & $llvmStrip --strip-all $soPath
}

# 4. Stage package
$stage = "target/apk_staging_$Target"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path "$stage/lib/$Target" -Force | Out-Null
Copy-Item $soPath "$stage/lib/$Target/libgroshe_android.so"

# 5. Link base APK with AndroidManifest.xml
Write-Host "Compiling manifest with aapt2 link..." -ForegroundColor Yellow
& $aapt2 link -o "$stage/base.zip" -I $androidJar --manifest $manifest
if ($LASTEXITCODE -ne 0) { throw "aapt2 link failed" }

# 6. Add native libraries into base.zip
Write-Host "Adding native libraries to APK..." -ForegroundColor Yellow
Push-Location $stage
try {
    Compress-Archive -Path "lib" -Update -DestinationPath "base.zip"
} finally {
    Pop-Location
}

# 7. 4-byte page align APK for memory-mapped library execution
$alignedApk = "target/groshe-$Profile-$Target.apk"
Write-Host "Aligning APK with zipalign..." -ForegroundColor Yellow
& $zipalign -p -f 4 "$stage/base.zip" $alignedApk
if ($LASTEXITCODE -ne 0) { throw "zipalign failed" }

# 8. Sign APK with v2/v3 schemes
Write-Host "Signing APK with apksigner..." -ForegroundColor Yellow
& $apksigner sign --ks $keystore --ks-pass pass:android --key-pass pass:android --out $alignedApk $alignedApk
if ($LASTEXITCODE -ne 0) { throw "apksigner failed" }

# 9. Verify package
$apkItem = Get-Item $alignedApk
$apkMb = [math]::Round($apkItem.Length / 1MB, 2)
Write-Host "`n[SUCCESS] Packaged APK: $alignedApk" -ForegroundColor Green
Write-Host "Package Size: $apkMb MB ($($apkItem.Length) bytes)" -ForegroundColor Green

if ($Profile -eq "release") {
    if ($apkItem.Length -lt 25MB) {
        Write-Host "Exit Criteria Passed: $apkMb MB < 25 MB target limit." -ForegroundColor Green
    } else {
        Write-Warning "APK exceeds 25 MB target limit: $apkMb MB."
    }
}
