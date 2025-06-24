//! Shared protobuf schemas for Bitquery Solana data

// Include the generated protobuf code from official Bitquery schemas
include!(concat!(env!("OUT_DIR"), "/solana_messages.rs"));
