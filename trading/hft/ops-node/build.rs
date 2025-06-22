fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../proto/");
    
    tonic_build::configure()
        .build_server(false)
        .out_dir("src/generated") // Explicit output directory
        .compile_protos(
            &[
                "../proto/yellowstone/geyser.proto",
                "../proto/jito/shredstream.proto",
            ],
            &["../proto"],
        )?;
    
    Ok(())
}
