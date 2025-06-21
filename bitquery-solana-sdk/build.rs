use std::io::Result;

fn main() -> Result<()> {
    // Compile protobuf schemas
    // Assuming the .proto file will be at src/schemas/solana.proto
    // and the output directory for prost-build is correctly picked up by include! macro.
    prost_build::Config::new()
        .bytes(&["."]) // Enable bytes for Bytes fields in proto
        .compile_protos(&["src/schemas/solana.proto"], &["src/schemas/"])?;
    Ok(())
}
