fn main() {
    println!("cargo:rerun-if-changed=ui/lumen.slint");

    slint_build::compile("ui/lumen.slint").expect("Failed to compile Slint UI");
}