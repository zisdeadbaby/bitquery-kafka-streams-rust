use std::io::Result;

fn main() -> Result<()> {
    // Unified protobuf compilation for all shared schemas
    prost_build::Config::new()
        .bytes(["."])  // Enable bytes for Bytes fields in proto
        .type_attribute(".", "#[allow(missing_docs)]") // Suppress missing docs warnings for generated code
        .field_attribute(".", "#[allow(missing_docs)]") // Suppress missing docs warnings for generated fields
        .compile_protos(&[
            "../schemas/block_message.proto",
            "../schemas/token_block_message.proto", 
            "../schemas/dex_block_message.proto",
            "../schemas/parsed_idl_block_message.proto"
        ], &["../schemas/"])?;
    Ok(())
}
