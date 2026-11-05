fn main() {
    // Gambar sprite didekode saat build, jadi exe tidak perlu dekoder PNG.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);
    slint_build::compile_with_config("ui/main.slint", config).expect("gagal mengompilasi UI Slint");
}
