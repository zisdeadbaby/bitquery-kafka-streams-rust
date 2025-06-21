use std::io::Result;

fn main() -> Result<()> {
    // Unified protobuf compilation for all shared schemas
    prost_build::Config::new()
        .bytes(&["."]) // Enable bytes for Bytes fields in proto
        .compile_protos(&["src/schemas/solana.proto"], &["src/schemas/"])?;
    Ok(())
}
