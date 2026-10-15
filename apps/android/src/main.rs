#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    groshe_android::run_desktop()
}

#[cfg(target_os = "android")]
fn main() {}
