use crate::error::{Error as SdkError, Result as SdkResult};
use tracing::error; // For logging detailed errors

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
/// A `SdkResult<Vec<u8>>` containing the decompressed data or an `SdkError`
/// if decompression fails. `SdkError::Compression` will contain specific details.
pub fn decompress_lz4(data: &[u8]) -> SdkResult<Vec<u8>> {
    if data.is_empty() {
        // lz4_flex::decompress_size_prepended returns an error for empty slice too,
        // but an explicit check can provide a clearer SDK error message.
        return Err(SdkError::Compression("Input data for LZ4 decompression cannot be empty.".to_string()));
    }

    lz4_flex::decompress_size_prepended(data)
        .map_err(|e| {
            // Log the detailed error for server-side diagnostics if helpful
            error!("LZ4 decompression failed internally: {}. Input data length: {}.", e, data.len());
            // Return a structured SDK error
            SdkError::Compression(format!("LZ4 decompression error: {}", e))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_lz4_valid_data() {
        // Sample data known to be compressible
        let original_data = b"The quick brown fox jumps over the lazy dog. Repeat: The quick brown fox jumps over the lazy dog.";
        // Compress it using lz4_flex with the size prepended (standard for Bitquery messages)
        let compressed_data = lz4_flex::compress_prepend_size(original_data);
        assert!(!compressed_data.is_empty(), "Compressed data should not be empty");

        // Attempt to decompress
        let decompressed_data = decompress_lz4(&compressed_data).expect("Decompression of valid data failed");

        // Verify that the decompressed data matches the original
        assert_eq!(decompressed_data, original_data, "Decompressed data does not match original");
    }

    #[test]
    fn test_decompress_lz4_empty_input() {
        let empty_data = [];
        let result = decompress_lz4(&empty_data);
        assert!(result.is_err(), "Decompressing empty data should result in an error");
        match result {
            Err(SdkError::Compression(msg)) => {
                assert!(msg.contains("Input data for LZ4 decompression cannot be empty"), "Error message mismatch for empty data");
            }
            _ => panic!("Expected SdkError::Compression for empty data input"),
        }
    }

    #[test]
    fn test_decompress_lz4_invalid_or_corrupted_data() {
        // Arbitrary byte sequence unlikely to be valid LZ4 compressed data
        let invalid_data = b"\x00\x00\x00\x00\xFF\xEE\xDD\xCC\xBB\xAA"; // Example of invalid data
        let result = decompress_lz4(invalid_data);
        assert!(result.is_err(), "Decompressing invalid data should result in an error");
        match result {
            Err(SdkError::Compression(msg)) => {
                // The exact internal error message from lz4_flex might vary based on the corruption.
                // We check that our wrapper correctly attributes it to a compression error.
                assert!(msg.starts_with("LZ4 decompression error:"), "Error message should indicate LZ4 decompression issue");
            }
            _ => panic!("Expected SdkError::Compression for invalid or corrupted data"),
        }
    }

    #[test]
    fn test_decompress_lz4_corrupted_size_prefix() {
        let original_data = b"Some moderately sized data to ensure compression happens and there's a prefix.";
        let mut compressed_data = lz4_flex::compress_prepend_size(original_data);

        // Ensure there's data to corrupt (at least the 4-byte size prefix)
        if compressed_data.len() >= 4 {
            // Corrupt the size prefix (e.g., make it claim the data is much larger or smaller)
            // For example, flip some bits in the first byte of the size.
            compressed_data[0] = compressed_data[0] ^ 0xFF;
        } else {
            // This case should ideally not happen for non-trivial original_data
            panic!("Compressed data is too short to test size prefix corruption meaningfully.");
        }

        let result = decompress_lz4(&compressed_data);
        assert!(result.is_err(), "Decompression should fail with a corrupted size prefix");
         match result {
            Err(SdkError::Compression(msg)) => {
                assert!(msg.starts_with("LZ4 decompression error:"), "Error message should indicate LZ4 issue for corrupted prefix");
            }
            _ => panic!("Expected SdkError::Compression for corrupted size prefix"),
        }
    }
}
