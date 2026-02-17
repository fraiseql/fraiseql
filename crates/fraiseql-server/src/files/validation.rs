//! File validation with magic bytes detection

use bytes::Bytes;

use crate::files::{
    config::{FileConfig, parse_size},
    error::FileError,
    traits::{FileValidator, ValidatedFile},
};

/// Default file validator implementation
pub struct DefaultFileValidator;

impl FileValidator for DefaultFileValidator {
    fn validate(
        &self,
        data: &Bytes,
        declared_type: &str,
        filename: &str,
        config: &FileConfig,
    ) -> Result<ValidatedFile, FileError> {
        validate_file(data, declared_type, filename, config)
    }
}

/// Validate uploaded file
pub fn validate_file(
    data: &Bytes,
    declared_type: &str,
    filename: &str,
    config: &FileConfig,
) -> Result<ValidatedFile, FileError> {
    // Check size
    let max_size = parse_size(&config.max_size).unwrap_or(10 * 1024 * 1024);

    if data.len() > max_size {
        return Err(FileError::TooLarge {
            size: data.len(),
            max:  max_size,
        });
    }

    // Check MIME type is allowed
    if !config.allowed_types.iter().any(|t| t == declared_type || t == "*/*") {
        return Err(FileError::InvalidType {
            got:     declared_type.to_string(),
            allowed: config.allowed_types.clone(),
        });
    }

    // Sanitize filename
    let sanitized = sanitize_filename(filename)?;

    // Detect content type if magic bytes validation is enabled
    let detected_type = if config.validate_magic_bytes {
        let detected = detect_content_type(data);
        validate_magic_bytes(&detected, declared_type)?;
        Some(detected)
    } else {
        None
    };

    Ok(ValidatedFile {
        content_type: declared_type.to_string(),
        sanitized_filename: sanitized,
        size: data.len(),
        detected_type,
    })
}

/// Detect content type from magic bytes
pub fn detect_content_type(data: &Bytes) -> String {
    infer::get(data)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Validate file content matches declared MIME type
fn validate_magic_bytes(detected: &str, declared: &str) -> Result<(), FileError> {
    // Allow some flexibility in MIME type matching
    if !mime_types_compatible(detected, declared) {
        return Err(FileError::MimeMismatch {
            declared: declared.to_string(),
            detected: detected.to_string(),
        });
    }

    Ok(())
}

fn mime_types_compatible(detected: &str, declared: &str) -> bool {
    // Exact match
    if detected == declared {
        return true;
    }

    // Common equivalents
    let equivalents = [
        ("image/jpeg", "image/jpg"),
        ("text/plain", "application/octet-stream"),
    ];

    for (a, b) in equivalents {
        if (detected == a && declared == b) || (detected == b && declared == a) {
            return true;
        }
    }

    // Same major type (e.g., image/*)
    let detected_major = detected.split('/').next().unwrap_or("");
    let declared_major = declared.split('/').next().unwrap_or("");

    // For images, allow any image type if major matches
    if detected_major == "image" && declared_major == "image" {
        return true;
    }

    false
}

/// Sanitize filename to prevent path traversal and other attacks
pub fn sanitize_filename(filename: &str) -> Result<String, FileError> {
    // Remove path components (prevent ../../../etc/passwd)
    let filename = filename.rsplit(['/', '\\']).next().unwrap_or(filename);

    // Empty filename after removing path
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(FileError::InvalidFilename {
            reason: "Filename cannot be empty or path component".into(),
        });
    }

    // Remove null bytes (C string terminator attack)
    let filename = filename.replace('\0', "");

    // Limit length
    if filename.len() > 255 {
        return Err(FileError::InvalidFilename {
            reason: "Filename too long (max 255 characters)".into(),
        });
    }

    // Replace dangerous characters but preserve extension
    let sanitized: String = filename
        .chars()
        .enumerate()
        .map(|(i, c)| {
            match c {
                // Allow alphanumeric
                'a'..='z' | 'A'..='Z' | '0'..='9' => c,
                // Allow dot (for extension) but not as first character
                '.' if i > 0 => c,
                // Allow hyphen and underscore
                '-' | '_' => c,
                // Replace everything else with underscore
                _ => '_',
            }
        })
        .collect();

    // Ensure we have a valid filename
    if sanitized.is_empty() || sanitized.chars().all(|c| c == '_') {
        return Err(FileError::InvalidFilename {
            reason: "Filename contains no valid characters".into(),
        });
    }

    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_compatibility() {
        assert!(mime_types_compatible("image/jpeg", "image/jpeg"));
        assert!(mime_types_compatible("image/jpeg", "image/jpg"));
        assert!(mime_types_compatible("image/png", "image/webp")); // Same major
        assert!(!mime_types_compatible("image/jpeg", "application/pdf"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("photo.jpg").unwrap(), "photo.jpg");
        assert_eq!(sanitize_filename("my-file_2024.pdf").unwrap(), "my-file_2024.pdf");

        // Path traversal
        let result = sanitize_filename("../../../etc/passwd").unwrap();
        assert!(!result.contains(".."));
        assert_eq!(result, "passwd");

        // Dangerous characters
        let result = sanitize_filename("file<>:\"|?*.jpg").unwrap();
        assert!(!result.contains('<'));
        assert!(!result.contains('>'));
        assert!(!result.contains(':'));
    }

    #[test]
    fn test_null_byte_removal() {
        let result = sanitize_filename("image.jpg\0.exe").unwrap();
        assert!(!result.contains('\0'));
    }
}
