fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../proto/");
    
    tonic_build::configure()
        .build_server(false)
        .out_dir("src/generated") // Explicit output directory
        .type_attribute(".", "#[allow(missing_docs)]") // Suppress missing docs warnings for generated code
        .field_attribute(".", "#[allow(missing_docs)]") // Suppress missing docs warnings for generated fields
        .compile_protos(
            &[
                "../proto/yellowstone/geyser.proto",
                "../proto/jito/shredstream.proto",
            ],
            &["../proto"],
        )?;
    
    Ok(())
}
