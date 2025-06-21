use std::io::Result;

fn main() -> Result<()> {
    // This script compiles Protocol Buffer definitions into Rust code.
    // It uses the `prost-build` crate, specified in `[build-dependencies]`.

    // Configure prost_build
    prost_build::Config::new()
        // Enable generation of `Bytes` type for `bytes` fields in .proto files.
        // This is often preferred for performance and to avoid excessive copying.
        .bytes(&["."]) // Apply to all fields, or specify paths like &["solana.Transaction.data"]
        // Specify the .proto files to compile and the include paths for dependencies.
        // The include path "&["src/schemas/"]" tells prost-build where to find `solana.proto`
        // if it had any `import` statements for other .proto files in the same directory.
        .compile_protos(
            &["src/schemas/solana.proto"], // List of .proto files to compile
            &["src/schemas/"],             // Include paths for .proto dependencies
        )?;

    Ok(())
}
