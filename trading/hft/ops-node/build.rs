use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto"); // Adjusted path for workspace structure

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(out_dir.join("grpc_generated")) // Output to src/grpc_generated
        .compile(
            &[
                "../../proto/yellowstone/geyser.proto", // Adjusted path
                "../../proto/jito/shredstream.proto",   // Adjusted path
            ],
            &["../../proto/"], // Adjusted path
        )?;

    Ok(())
}
