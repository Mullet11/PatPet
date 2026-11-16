fn main() {
    for file in ["assets/patpet.rc", "assets/app.ico", "assets/patpet.manifest", "assets/pet"] {
        println!("cargo:rerun-if-changed={file}");
    }
    // exe / tray icon (ICON 1) and the application manifest (RT_MANIFEST 1)
    embed_resource::compile("assets/patpet.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to compile resources");
}
