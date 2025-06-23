use crate::error::{Error, Result};
use flate2::read::GzDecoder;
use std::io::Read;

/// Attempts to decompress LZ4 compressed data that is prefixed with its original size.
///
/// Kafka messages from Bitquery may be LZ4 compressed. This function handles
/// the decompression, expecting the standard LZ4 block format with a prepended
/// size of the original uncompressed data.
///
/// # Arguments
/// * `data`: A byte slice containing potentially LZ4 compressed data.
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

    // Check if data is likely LZ4 compressed by examining magic bytes and structure
    if !is_likely_lz4_compressed(data) {
        return Err(Error::Compression(
            "Data does not appear to be LZ4 compressed".to_string()
        ));
    }

    lz4_flex::decompress_size_prepended(data)
        .map_err(|lz4_error| {
            // Log at debug level instead of error to reduce noise
            tracing::debug!(
                "LZ4 decompression failed. Error: '{}'. Input data length: {}.",
                lz4_error, data.len()
            );
            Error::Compression(format!("LZ4 decompression error: {}", lz4_error))
        })
}

/// Attempts to safely decompress data, trying multiple decompression methods.
///
/// This function first checks if the data appears to be compressed and attempts
/// appropriate decompression. If no compression is detected or decompression fails,
/// it returns the original data.
///
/// # Arguments
/// * `data`: A byte slice containing potentially compressed data.
///
/// # Returns
/// A `Vec<u8>` containing either decompressed data or the original data if
/// decompression was not possible or necessary.
pub fn decompress_safe(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return data.to_vec();
    }

    // Log the first few bytes for debugging compression detection
    if data.len() >= 4 {
        tracing::trace!("Decompress_safe: examining data, length: {}, first 4 bytes: {:02x} {:02x} {:02x} {:02x}", 
                       data.len(), data[0], data[1], data[2], data[3]);
    }

    // Try LZ4 decompression first (only if it looks like LZ4)
    if is_likely_lz4_compressed(data) {
        tracing::trace!("Decompress_safe: attempting LZ4 decompression");
        if let Ok(decompressed) = decompress_lz4(data) {
            tracing::trace!("Successfully decompressed LZ4 data: {} -> {} bytes", data.len(), decompressed.len());
            return decompressed;
        } else {
            tracing::trace!("LZ4 decompression failed, continuing with other methods");
        }
    } else {
        tracing::trace!("Data does not appear to be LZ4 compressed");
    }

    // Try gzip decompression if LZ4 fails (only if it looks like gzip)
    if is_likely_gzip_compressed(data) {
        tracing::trace!("Decompress_safe: attempting gzip decompression");
        if let Ok(decompressed) = decompress_gzip(data) {
            tracing::trace!("Successfully decompressed gzip data: {} -> {} bytes", data.len(), decompressed.len());
            return decompressed;
        } else {
            tracing::trace!("Gzip decompression failed, using raw data");
        }
    } else {
        tracing::trace!("Data does not appear to be gzip compressed");
    }

    // If all decompression attempts fail, return original data
    tracing::trace!("No decompression applied, using raw data: {} bytes", data.len());
    data.to_vec()
}

/// Checks if data is likely gzip compressed by examining magic bytes and header structure.
///
/// Gzip files start with a specific magic number (0x1f, 0x8b) followed by compression method,
/// flags, timestamp, extra flags, and OS identifier. This function performs more thorough
/// validation to avoid false positives.
fn is_likely_gzip_compressed(data: &[u8]) -> bool {
    // Need at least 10 bytes for a minimal gzip header
    if data.len() < 10 {
        return false;
    }
    
    // Check magic bytes (0x1f, 0x8b)
    if data[0] != 0x1f || data[1] != 0x8b {
        return false;
    }
    
    // Check compression method (should be 8 for deflate)
    if data[2] != 8 {
        return false;
    }
    
    // Check flags byte (bits 3-7 should be 0, reserved for future use)
    let flags = data[3];
    if (flags & 0xe0) != 0 {
        return false;
    }
    
    // Additional heuristic: very small payloads (< 20 bytes total) are unlikely to be valid gzip
    // since gzip has significant header overhead
    if data.len() < 20 {
        return false;
    }
    
    // If we reach here, the header structure looks valid for gzip
    true
}

/// Checks if data is likely LZ4 compressed by examining its structure.
///
/// This performs basic heuristics to avoid attempting decompression on
/// data that's clearly not LZ4 compressed.
fn is_likely_lz4_compressed(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }

    // Check for size prefix (first 4 bytes should be a reasonable uncompressed size)
    let size_prefix = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    
    // Basic sanity checks:
    // 1. Uncompressed size should not be zero
    // 2. Don't attempt decompression if claimed uncompressed size is unreasonably large (>100MB)
    if size_prefix == 0 || size_prefix > 100 * 1024 * 1024 {
        return false;
    }
    
    // Additional heuristic: very small payloads (< 16 bytes total) are unlikely to have LZ4 header + data
    if data.len() < 16 {
        return false;
    }
    
    // More strict validation: the uncompressed size should be reasonable compared to compressed size
    // For most real data, compression ratio should be between 0.1 and 10.0
    let compressed_size = data.len() - 4; // Subtract 4 bytes for the size prefix
    let compression_ratio = size_prefix as f64 / compressed_size as f64;
    
    // If the claimed uncompressed size is more than 10x the compressed size, it's suspicious
    // If the claimed uncompressed size is less than 10% of compressed size, also suspicious
    if compression_ratio > 10.0 || compression_ratio < 0.1 {
        return false;
    }

    true
}

/// Decompresses gzip compressed data.
///
/// # Arguments
/// * `data`: A byte slice containing the gzip compressed data.
///
/// # Returns
/// A `Result<Vec<u8>>` containing the decompressed data or an `Error`
/// if decompression fails.
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(Error::Compression(
            "Input data for gzip decompression cannot be empty.".to_string()
        ));
    }

    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    
    decoder.read_to_end(&mut decompressed)
        .map_err(|gzip_error| {
            // Log at debug level instead of error to reduce noise
            tracing::debug!(
                "Gzip decompression failed. Error: '{}'. Input data length: {}.",
                gzip_error, data.len()
            );
            Error::Compression(format!("Gzip decompression error: {}", gzip_error))
        })?;
    
    Ok(decompressed)
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
                    message.contains("Data does not appear to be LZ4 compressed") || 
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
                    message.contains("Data does not appear to be LZ4 compressed") || 
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

    #[test]
    fn test_decompress_gzip_with_valid_data() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original_data = b"This is a test string for gzip compression.";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original_data).expect("Failed to write data");
        let compressed_data = encoder.finish().expect("Failed to compress data");

        assert!(!compressed_data.is_empty(), "Compressed data should not be empty.");

        let decompressed_data_result = decompress_gzip(&compressed_data);
        assert!(decompressed_data_result.is_ok(), "Decompression of valid gzip data failed: {:?}", decompressed_data_result.err());

        assert_eq!(decompressed_data_result.unwrap(), original_data, "Decompressed data does not match the original data.");
    }

    #[test]
    fn test_decompress_gzip_with_empty_input() {
        let empty_data = &[];
        let result = decompress_gzip(empty_data);

        assert!(result.is_err(), "Decompressing an empty data slice should result in an error.");
        match result {
            Err(Error::Compression(message)) => {
                assert!(
                    message.contains("Input data for gzip decompression cannot be empty"),
                    "Error message for empty data input is not as expected. Got: '{}'", message
                );
            }
            _ => panic!("Expected Error::Compression when decompressing empty data, but got a different error or Ok."),
        }
    }

    #[test]
    fn test_decompress_gzip_with_invalid_data() {
        let invalid_data = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F";
        let result = decompress_gzip(invalid_data);

        assert!(result.is_err(), "Decompressing invalid/corrupted gzip data should result in an error.");
        match result {
            Err(Error::Compression(message)) => {
                assert!(
                    message.starts_with("Gzip decompression error:"),
                    "Error message should indicate a Gzip decompression issue. Got: '{}'", message
                );
            }
            _ => panic!("Expected Error::Compression for invalid/corrupted gzip data, but got a different error or Ok."),
        }
    }

    #[test]
    fn test_decompress_safe_with_lz4_data() {
        let original_data = b"This is a test string that will be compressed and then decompressed safely.";
        let compressed_data = lz4_flex::compress_prepend_size(original_data);

        let result = decompress_safe(&compressed_data);
        assert_eq!(result, original_data, "Safe decompression should successfully decompress LZ4 data.");
    }

    #[test]
    fn test_decompress_safe_with_uncompressed_data() {
        let original_data = b"This is uncompressed data that should be returned as-is.";
        
        let result = decompress_safe(original_data);
        assert_eq!(result, original_data, "Safe decompression should return uncompressed data as-is.");
    }

    #[test]
    fn test_decompress_safe_with_empty_data() {
        let empty_data = b"";
        
        let result = decompress_safe(empty_data);
        assert_eq!(result, empty_data, "Safe decompression should handle empty data gracefully.");
    }

    #[test]
    fn test_decompress_safe_with_invalid_data() {
        let invalid_data = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F";
        
        let result = decompress_safe(invalid_data);
        assert_eq!(result, invalid_data, "Safe decompression should return invalid data as-is when decompression fails.");
    }

    #[test]
    fn test_is_likely_lz4_compressed() {
        // Test with actual LZ4 compressed data
        let original_data = b"This is a reasonably long test string that should compress well with LZ4 algorithm.";
        let compressed_data = lz4_flex::compress_prepend_size(original_data);
        assert!(is_likely_lz4_compressed(&compressed_data), "Valid LZ4 data should be detected as LZ4 compressed.");

        // Test with too small data
        let small_data = b"small";
        assert!(!is_likely_lz4_compressed(small_data), "Very small data should not be considered LZ4 compressed.");

        // Test with empty data
        let empty_data = b"";
        assert!(!is_likely_lz4_compressed(empty_data), "Empty data should not be considered LZ4 compressed.");

        // Test with data that has unreasonable size prefix (too large)
        let invalid_size_data = b"\xFF\xFF\xFF\xFF\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F\x10";
        assert!(!is_likely_lz4_compressed(invalid_size_data), "Data with unreasonably large size prefix should not be considered LZ4 compressed.");
        
        // Test with zero size prefix
        let zero_size_data = b"\x00\x00\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F\x10";
        assert!(!is_likely_lz4_compressed(zero_size_data), "Data with zero size prefix should not be considered LZ4 compressed.");
    }
}
