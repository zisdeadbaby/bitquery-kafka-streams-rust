// This file is intentionally left blank or with minimal content.
// It will be populated by the `tonic_build` process specified in `build.rs`.
// The build script is configured to output generated files into a directory
// that corresponds to this module's path relative to OUT_DIR.

// Example of how includes might look if files were directly here (for reference):
// pub mod yellowstone {
//     tonic::include_proto!("yellowstone"); // Assumes yellowstone.rs is generated in OUT_DIR/yellowstone.rs
// }
// pub mod jito {
//     tonic::include_proto!("jito"); // Assumes jito.rs is generated in OUT_DIR/jito.rs
// }

// The build.rs script is configured with:
// .out_dir(out_dir.join("grpc_generated"))
// This means tonic-build will create files like:
// $OUT_DIR/grpc_generated/yellowstone.rs
// $OUT_DIR/grpc_generated/jito.rs

// And grpc_client.rs will use:
// mod proto {
//     pub mod yellowstone {
//         tonic::include_proto!("grpc_generated/yellowstone"); // Points to $OUT_DIR/grpc_generated/yellowstone.rs
//     }
//     pub mod jito {
//         tonic::include_proto!("grpc_generated/jito"); // Points to $OUT_DIR/grpc_generated/jito.rs
//     }
// }
// So, this mod.rs file itself doesn't need to declare submodules for yellowstone and jito,
// as they are directly included by grpc_client.rs from the OUT_DIR.

// If you wanted this mod.rs to re-export them, you would do it differently,
// but current setup in grpc_client.rs handles it.

// We can remove the .gitkeep file now.
// No specific code is needed here due to the way build.rs and grpc_client.rs are set up.
