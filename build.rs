use prost_build::Config;
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only run if we have proto files
    let proto_dir = PathBuf::from("schemas");
    if !proto_dir.exists() {
        println!("cargo:warning=No schemas directory found, skipping protobuf generation");
        return Ok(());
    }

    let proto_files = std::fs::read_dir(&proto_dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "proto" {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if proto_files.is_empty() {
        println!("cargo:warning=No .proto files found in schemas directory");
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let mut config = Config::new();
    config.out_dir(&out_dir);

    // Configure package mappings and type attributes as needed
    config.compile_protos(
        &proto_files.iter().map(|p| p.as_path()).collect::<Vec<_>>(),
        &[&proto_dir],
    )?;

    println!("cargo:rerun-if-changed=schemas/");
    for proto_file in &proto_files {
        println!("cargo:rerun-if-changed={}", proto_file.display());
    }

    Ok(())
}
