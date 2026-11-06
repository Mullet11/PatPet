fn main() {
    for file in ["assets/patpet.rc", "assets/app.ico", "assets/patpet.manifest"] {
        println!("cargo:rerun-if-changed={file}");
    }

    // Ikon exe dan tray (ICON 1) serta manifest aplikasi (RT_MANIFEST 1).
    embed_resource::compile("assets/patpet.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("gagal mengompilasi resource");

    // Gambar sprite didekode saat build, jadi exe tidak perlu dekoder PNG.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);
    slint_build::compile_with_config("ui/main.slint", config).expect("gagal mengompilasi UI Slint");
}
