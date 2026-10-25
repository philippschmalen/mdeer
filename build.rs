//! Embeds the icon and version info into `mdeer.exe` (Windows targets only).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/mdeer.ico")
            .set("FileDescription", "MDeer markdown reader and editor")
            .set("ProductName", "MDeer");
        res.compile().expect("embed Windows resources");
    }
}
