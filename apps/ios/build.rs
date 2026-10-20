fn main() {
    slint_build::compile("../android/ui/app.slint")
        .expect("Failed to compile Slint UI templates for iOS");
}
