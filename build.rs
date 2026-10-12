fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
    // The exe's icon (Explorer, the Run list, Task Manager).
    let mut res = winresource::WindowsResource::new();
    res.set_icon("ui/icon.ico").set("ProductName", "Helm").set("FileDescription", "Helm");
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed the exe icon: {e}");
    }
}
