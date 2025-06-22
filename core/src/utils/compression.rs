use crate::error::{Error, Result};
use tracing::error;

/// Decompresses LZ4 compressed data that is prefixed with its original size.
///
/// Kafka messages from Bitquery are often LZ4 compressed. This function handles
/// the decompression, expecting the standard LZ4 block format with a prepended
/// size of the original uncompressed data.
///
/// # Arguments
/// * `data`: A byte slice containing the LZ4 compressed data with a size prefix.
///
/// # Returns
/// A `Result<Vec<u8>>` containing the decompressed data or an `Error`
/// if decompression fails.
pub fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(Error::Compression(
            "Input data for LZ4 decompression cannot be empty.".to_string()
        ));
    }

    lz4_flex::decompress_size_prepended(data)
        .map_err(|lz4_error| {
            error!(
                "LZ4 decompression failed internally. Error: '{}'. Input data length: {}.",
                lz4_error, data.len()
            );
            Error::Compression(format!("LZ4 decompression error: {}", lz4_error))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_lz4_with_valid_data() {
        let original_data = b"This is a test string that will be compressed and then decompressed. It needs to be reasonably long for compression to be effective.";
        let compressed_data = lz4_flex::compress_prepend_size(original_data);

        assert!(!compressed_data.is_empty(), "Compressed data should not be empty.");

        let decompressed_data_result = decompress_lz4(&compressed_data);
        assert!(decompressed_data_result.is_ok(), "Decompression of valid data failed: {:?}", decompressed_data_result.err());

        assert_eq!(decompressed_data_result.unwrap(), original_data, "Decompressed data does not match the original data.");
    }

    #[test]
    fn test_decompress_lz4_with_empty_input() {
        let empty_data = &[];
        let result = decompress_lz4(empty_data);

        assert!(result.is_err(), "Decompressing an empty data slice should result in an error.");
        match result {
            Err(Error::Compression(message)) => {
                assert!(
                    message.contains("Input data for LZ4 decompression cannot be empty"),
                    "Error message for empty data input is not as expected. Got: '{}'", message
                );
            }
            _ => panic!("Expected Error::Compression when decompressing empty data, but got a different error or Ok."),
        }
    }

    #[test]
    fn test_decompress_lz4_with_invalid_data() {
        let invalid_data = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F";
        let result = decompress_lz4(invalid_data);

        assert!(result.is_err(), "Decompressing invalid/corrupted data should result in an error.");
        match result {
            Err(Error::Compression(message)) => {
                assert!(
                    message.starts_with("LZ4 decompression error:"),
                    "Error message should indicate an LZ4 decompression issue. Got: '{}'", message
                );
            }
            _ => panic!("Expected Error::Compression for invalid/corrupted data, but got a different error or Ok."),
        }
    }

    #[test]
    fn test_decompress_lz4_with_corrupted_size_prefix() {
        let original_data = b"Data for testing corrupted size prefix. This needs to be long enough for meaningful compression.";
        let mut compressed_data = lz4_flex::compress_prepend_size(original_data);

        // Overwrite the size prefix with an obviously invalid value (e.g., all 0xFF)
        if compressed_data.len() >= 4 {
            for i in 0..4 {
                compressed_data[i] = 0xFF;
            }
        } else {
            panic!("Compressed data is too short (length {}) to reliably test size prefix corruption.", compressed_data.len());
        }

        let result = decompress_lz4(&compressed_data);
        match result {
            Err(Error::Compression(message)) => {
                assert!(
                    message.starts_with("LZ4 decompression error:"),
                    "Error message for corrupted size prefix should indicate an LZ4 issue. Got: '{}'", message
                );
            }
            Ok(decompressed) => {
                // Accept as long as the decompressed data does not match the original
                assert_ne!(decompressed, original_data, "Corrupted size prefix should not yield original data");
            }
            _ => panic!("Expected Error::Compression or incorrect decompression for corrupted size prefix."),
        }
    }
}
