fn main() {
    unsafe { std::env::set_var("RUST_BACKTRACE", "full") };

    slint_build::compile("ui/base.slint").unwrap();
}