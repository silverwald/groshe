# Groshe Task Runner

# On Windows, just defaults to `sh` which is usually absent. Use [windows] attribute to set powershell.exe.
[windows]
set shell := ["powershell.exe", "-NoLogo", "-Command"]

# Android SDK, NDK, and Java paths (auto-fallback to standard local installation paths)
sdk_dir := env_var_or_default("LOCALAPPDATA", "") + "/Android/Sdk"
jdk_dir := env_var_or_default("USERPROFILE", "") + "/.jdks/jdk-17.0.20.1+1"

export ANDROID_HOME := env_var_or_default("ANDROID_HOME", sdk_dir)
export ANDROID_SDK_ROOT := env_var_or_default("ANDROID_SDK_ROOT", sdk_dir)
export ANDROID_NDK_HOME := env_var_or_default("ANDROID_NDK_HOME", sdk_dir + "/ndk/android-ndk-r26b")
export ANDROID_NDK_ROOT := env_var_or_default("ANDROID_NDK_ROOT", sdk_dir + "/ndk/android-ndk-r26b")
export ANDROID_JAR := env_var_or_default("ANDROID_JAR", sdk_dir + "/platforms/android-34/android.jar")
export JAVA_HOME := env_var_or_default("JAVA_HOME", jdk_dir)

default: check test

# Check all workspace targets
check:
    cargo check --workspace --all-targets

# Run all workspace unit and integration tests
test:
    cargo test --workspace

# Build all workspace binaries and libraries
build:
    cargo build --workspace

# Format code with rustfmt
fmt:
    cargo fmt --all

# Lint with clippy
lint:
    cargo clippy --workspace --all-targets

# Run the CLI binary
cli *ARGS:
    cargo run -p groshe-cli -- {{ ARGS }}

# Test core domain crate only
test-core:
    cargo test -p groshe-core

# Build Android native libraries for 64-bit ARM and x86_64 emulator
android-build PROFILE="debug":
    cargo ndk -t arm64-v8a -t x86_64 build -p groshe-android {{ if PROFILE == "release" { "--release" } else { "" } }}

# Launch the interactive Slint mobile preview
android-preview:
    cargo run -p groshe-android
