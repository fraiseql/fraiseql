//! File upload handler

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::files::config::FileConfig;
use crate::files::error::{FileError, ProcessingError, ScanError, StorageError};
use crate::files::processing::ImageProcessorImpl;
use crate::files::traits::{FileValidator, ImageProcessor, MalwareScanner, StorageBackend};
use crate::files::validation::DefaultFileValidator;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileResponse {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub original_filename: Option<String>,
    pub content_type: String,
    pub size: u64,
    pub url: String,
    pub variants: Option<HashMap<String, String>>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SignedUrlResponse {
    pub url: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub enum HandlerError {
    File(FileError),
    Storage(StorageError),
    Processing(ProcessingError),
    Scan(ScanError),
}

impl From<FileError> for HandlerError {
    fn from(e: FileError) -> Self {
        Self::File(e)
    }
}

impl From<StorageError> for HandlerError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

impl From<ProcessingError> for HandlerError {
    fn from(e: ProcessingError) -> Self {
        Self::Processing(e)
    }
}

impl From<ScanError> for HandlerError {
    fn from(e: ScanError) -> Self {
        Self::Scan(e)
    }
}

pub struct FileHandler {
    upload_type: String,
    config: FileConfig,
    storage: Arc<dyn StorageBackend>,
    validator: Arc<dyn FileValidator>,
    processor: Option<Arc<dyn ImageProcessor>>,
    scanner: Option<Arc<dyn MalwareScanner>>,
}

impl FileHandler {
    pub fn new(
        upload_type: &str,
        config: FileConfig,
        storage: Arc<dyn StorageBackend>,
    ) -> Self {
        let validator = Arc::new(DefaultFileValidator) as Arc<dyn FileValidator>;
        let processor = config.processing.as_ref().map(|p| {
            Arc::new(ImageProcessorImpl::new(p.clone())) as Arc<dyn ImageProcessor>
        });

        Self {
            upload_type: upload_type.to_string(),
            config,
            storage,
            validator,
            processor,
            scanner: None,
        }
    }

    pub fn with_validator(mut self, validator: Arc<dyn FileValidator>) -> Self {
        self.validator = validator;
        self
    }

    pub fn with_scanner(mut self, scanner: Arc<dyn MalwareScanner>) -> Self {
        self.scanner = Some(scanner);
        self
    }

    pub async fn upload(
        &self,
        original_filename: &str,
        content_type: &str,
        data: Bytes,
        metadata: Option<serde_json::Value>,
    ) -> Result<FileResponse, HandlerError> {
        // Validate file
        let validated = self.validator.validate(
            &data,
            content_type,
            original_filename,
            &self.config,
        )?;

        // Scan for malware if configured
        if self.config.scan_malware {
            if let Some(scanner) = &self.scanner {
                let scan_result = scanner.scan(&data).await?;
                if !scan_result.clean {
                    return Err(FileError::MalwareDetected {
                        threat_name: scan_result.threat_name.unwrap_or_else(|| "Unknown".to_string()),
                    }
                    .into());
                }
            }
        }

        // Generate unique filename
        let file_id = uuid::Uuid::new_v4();
        let extension = validated
            .sanitized_filename
            .rsplit('.')
            .next()
            .unwrap_or("bin");
        let filename = format!("{}.{}", file_id, extension);

        // Storage key
        let storage_key = format!("{}/{}", self.upload_type, filename);

        // Process image if applicable
        let (variants, processed_data) = if self.is_image(content_type) {
            if let Some(processor) = &self.processor {
                let processing_config = self.config.processing.as_ref().unwrap();
                let processed = processor.process(&data, processing_config).await?;

                let mut variant_urls = HashMap::new();

                // Upload variants
                for (variant_name, variant_data) in &processed.variants {
                    if variant_name == "original" {
                        continue; // Skip original, we'll upload it separately
                    }

                    let variant_key = format!(
                        "{}/{}_{}.{}",
                        self.upload_type,
                        file_id,
                        variant_name,
                        self.get_output_extension()
                    );

                    let result = self
                        .storage
                        .upload(
                            &variant_key,
                            variant_data.clone(),
                            &self.get_output_content_type(),
                            None,
                        )
                        .await?;

                    variant_urls.insert(variant_name.clone(), result.url);
                }

                (
                    Some(variant_urls),
                    processed
                        .variants
                        .get("original")
                        .cloned()
                        .unwrap_or(data.clone()),
                )
            } else {
                (None, data.clone())
            }
        } else {
            (None, data.clone())
        };

        // Upload main file
        let upload_result = self
            .storage
            .upload(&storage_key, processed_data.clone(), content_type, None)
            .await?;

        // Create file record
        let file_record = FileResponse {
            id: file_id.to_string(),
            name: self.upload_type.clone(),
            filename,
            original_filename: Some(validated.sanitized_filename),
            content_type: content_type.to_string(),
            size: processed_data.len() as u64,
            url: upload_result.url,
            variants,
            metadata,
            created_at: chrono::Utc::now(),
        };

        Ok(file_record)
    }

    pub async fn signed_url(
        &self,
        storage_key: &str,
        expiry: Duration,
    ) -> Result<SignedUrlResponse, HandlerError> {
        let url = self.storage.signed_url(storage_key, expiry).await?;

        Ok(SignedUrlResponse {
            url,
            expires_at: chrono::Utc::now()
                + chrono::Duration::from_std(expiry).map_err(|e| {
                    StorageError::Provider {
                        message: e.to_string(),
                    }
                })?,
        })
    }

    pub async fn delete(&self, storage_key: &str) -> Result<(), HandlerError> {
        self.storage.delete(storage_key).await?;
        Ok(())
    }

    pub async fn exists(&self, storage_key: &str) -> Result<bool, HandlerError> {
        let exists = self.storage.exists(storage_key).await?;
        Ok(exists)
    }

    fn is_image(&self, content_type: &str) -> bool {
        content_type.starts_with("image/")
    }

    fn get_output_content_type(&self) -> String {
        match self
            .config
            .processing
            .as_ref()
            .and_then(|p| p.output_format.as_deref())
        {
            Some("webp") => "image/webp".to_string(),
            Some("png") => "image/png".to_string(),
            _ => "image/jpeg".to_string(),
        }
    }

    fn get_output_extension(&self) -> &str {
        match self
            .config
            .processing
            .as_ref()
            .and_then(|p| p.output_format.as_deref())
        {
            Some("webp") => "webp",
            Some("png") => "png",
            _ => "jpg",
        }
    }
}
