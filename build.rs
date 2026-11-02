fn main() {
    println!("cargo:rerun-if-changed=assets/aria.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let _ = embed_resource::compile("assets/aria.rc", embed_resource::NONE);
    }
}
