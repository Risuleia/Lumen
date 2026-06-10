fn main() {
    println!("cargo:rerun-if-changed=ui/lumen.slint");

    slint_build::compile("ui/Shell.slint").expect("Failed to compile Slint UI");
}