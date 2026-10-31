fn main() {
    // The font and the covers go into the binary as files; the software
    // renderer reads the font as vectors at run time (any size).
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/zone.slint", config).expect("compile ui/zone.slint");
}
