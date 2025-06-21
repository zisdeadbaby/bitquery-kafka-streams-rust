use crate::error::{Error as SdkError, Result as SdkResult};
use tracing::error; // For logging detailed errors if necessary

/// Decompresses LZ4 compressed data that is expected to be prefixed with its original,
/// uncompressed size. This format is common for Bitquery Kafka messages.
///
/// # Arguments
/// * `data`: A byte slice (`&[u8]`) containing the LZ4 compressed data,
///           which must include the prepended size of the original data.
///
/// # Returns
/// A `SdkResult<Vec<u8>>` which is:
/// - `Ok(Vec<u8>)` containing the decompressed data if successful.
/// - `Err(SdkError::Compression)` if decompression fails for any reason (e.g.,
///   empty input, corrupted data, incorrect format). The error message will
///   provide more details.
pub fn decompress_lz4(data: &[u8]) -> SdkResult<Vec<u8>> {
    if data.is_empty() {
        // lz4_flex::decompress_size_prepended itself would return an error for an empty slice,
        // but checking explicitly allows for a more specific SDK error message.
        return Err(SdkError::Compression(
            "Input data for LZ4 decompression cannot be empty.".to_string()
        ));
    }

    lz4_flex::decompress_size_prepended(data)
        .map_err(|lz4_error| {
            // Log the internal error for more detailed server-side diagnostics if needed.
            error!(
                "LZ4 decompression failed internally. Error: '{}'. Input data length: {}.",
                lz4_error, data.len()
            );
            // Wrap the lz4_flex error into the SDK's specific compression error type.
            SdkError::Compression(format!("LZ4 decompression error: {}", lz4_error))
        })
}

#[cfg(test)]
mod tests {
    use super::*; // Import parent module items, including decompress_lz4

    /// Tests successful decompression of valid LZ4 compressed data (with size prepended).
    #[test]
    fn test_decompress_lz4_with_valid_data() {
        let original_data = b"This is a test string that will be compressed and then decompressed. It needs to be reasonably long for compression to be effective.";
        // Compress the data using lz4_flex, prepending the original size.
        let compressed_data = lz4_flex::compress_prepend_size(original_data);

        // Ensure compression actually produced some data.
        assert!(!compressed_data.is_empty(), "Compressed data should not be empty.");

        // Attempt to decompress the data.
        let decompressed_data_result = decompress_lz4(&compressed_data);
        assert!(decompressed_data_result.is_ok(), "Decompression of valid data failed: {:?}", decompressed_data_result.err());

        // Verify that the decompressed data matches the original data.
        assert_eq!(decompressed_data_result.unwrap(), original_data, "Decompressed data does not match the original data.");
    }

    /// Tests the behavior of `decompress_lz4` when provided with an empty byte slice.
    #[test]
    fn test_decompress_lz4_with_empty_input() {
        let empty_data = &[]; // Empty slice
        let result = decompress_lz4(empty_data);

        assert!(result.is_err(), "Decompressing an empty data slice should result in an error.");
        match result {
            Err(SdkError::Compression(message)) => {
                assert!(
                    message.contains("Input data for LZ4 decompression cannot be empty"),
                    "Error message for empty data input is not as expected. Got: '{}'", message
                );
            }
            _ => panic!("Expected SdkError::Compression when decompressing empty data, but got a different error or Ok."),
        }
    }

    /// Tests `decompress_lz4` with data that is not valid LZ4 format or is corrupted.
    #[test]
    fn test_decompress_lz4_with_invalid_or_corrupted_data() {
        // Create an arbitrary byte sequence that is highly unlikely to be valid LZ4 data.
        let invalid_data = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F";
        let result = decompress_lz4(invalid_data);

        assert!(result.is_err(), "Decompressing invalid/corrupted data should result in an error.");
        match result {
            Err(SdkError::Compression(message)) => {
                // The exact error message from the lz4_flex crate can vary depending on the nature
                // of the corruption. We primarily check that our wrapper correctly identifies it as
                // a compression-related error.
                assert!(
                    message.starts_with("LZ4 decompression error:"),
                    "Error message should indicate an LZ4 decompression issue. Got: '{}'", message
                );
            }
            _ => panic!("Expected SdkError::Compression for invalid/corrupted data, but got a different error or Ok."),
        }
    }

    /// Tests `decompress_lz4` specifically with a corrupted size prefix.
    #[test]
    fn test_decompress_lz4_with_corrupted_size_prefix() {
        let original_data = b"Data for testing corrupted size prefix. This needs to be long enough.";
        let mut compressed_data = lz4_flex::compress_prepend_size(original_data);

        // Ensure there's enough data to corrupt (lz4_flex typically uses a 4-byte prefix for size).
        if compressed_data.len() >= 4 {
            // Corrupt the size prefix by altering its first byte.
            // This might make the prepended size invalid or inconsistent with the actual compressed data.
            compressed_data[0] = compressed_data[0].wrapping_add(1); // Increment to change the size
            // Or, for a more drastic change: compressed_data[0] = 0xFF;
        } else {
            // This case should ideally not be hit if `original_data` is sufficiently large.
            panic!("Compressed data is too short (length {}) to reliably test size prefix corruption.", compressed_data.len());
        }

        let result = decompress_lz4(&compressed_data);
        assert!(result.is_err(), "Decompression should fail if the size prefix is corrupted.");
         match result {
            Err(SdkError::Compression(message)) => {
                assert!(
                    message.starts_with("LZ4 decompression error:"),
                    "Error message for corrupted size prefix should indicate an LZ4 issue. Got: '{}'", message
                );
            }
            _ => panic!("Expected SdkError::Compression for corrupted size prefix, but got a different error or Ok."),
        }
    }
}
