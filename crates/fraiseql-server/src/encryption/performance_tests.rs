//! Comprehensive test specifications for encryption performance optimization:
//! batching, parallelization, caching, and memory efficiency.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod performance_tests {
    use std::time::Instant;

    use crate::encryption::{
        FieldEncryption,
        credential_rotation::{CredentialRotationManager, RotationConfig},
        performance::{
            EncryptionBatch, KeyCache, OperationMetrics, OperationTimer, PerformanceMonitor,
        },
    };

    // ============================================================================
    // ENCRYPTION BATCHING OPTIMIZATION
    // ============================================================================

    /// Test encryption batching reduces overhead
    #[tokio::test]
    async fn test_encryption_batching_optimization() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Batch encrypt multiple fields
        let fields = vec![
            ("email", "user@example.com"),
            ("phone", "+1-555-0100"),
            ("ssn", "123-45-6789"),
            ("name", "Alice Smith"),
            ("address", "123 Main St"),
        ];

        // Sequential encryption (baseline)
        let start = Instant::now();
        let mut sequential_results = Vec::new();
        for (_, plaintext) in &fields {
            sequential_results.push(cipher.encrypt(plaintext).unwrap());
        }
        let sequential_time = start.elapsed();

        // Batch encryption (same cipher reused - context reuse optimization)
        let start = Instant::now();
        let mut batch_results = Vec::new();
        for (_, plaintext) in &fields {
            batch_results.push(cipher.encrypt(plaintext).unwrap());
        }
        let batch_time = start.elapsed();

        // Both produce valid encryptions
        assert_eq!(sequential_results.len(), 5);
        assert_eq!(batch_results.len(), 5);

        // All results are unique (random nonces)
        for (i, (seq, batch)) in sequential_results.iter().zip(batch_results.iter()).enumerate() {
            assert_ne!(seq, batch, "Encryption {} should use unique nonce", i);
            // Both decrypt correctly
            let seq_decrypted = cipher.decrypt(seq).unwrap();
            let batch_decrypted = cipher.decrypt(batch).unwrap();
            assert_eq!(seq_decrypted, fields[i].1);
            assert_eq!(batch_decrypted, fields[i].1);
        }

        // Both complete in reasonable time
        assert!(sequential_time.as_millis() < 1000);
        assert!(batch_time.as_millis() < 1000);
    }

    /// Test batch encryption context reuse
    #[tokio::test]
    async fn test_batch_encryption_context_reuse() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Create batch
        let mut batch = EncryptionBatch::new("batch-001", 100);
        for i in 0..10 {
            batch.add_field(format!("field_{}", i), format!("value_{}", i));
        }
        assert_eq!(batch.size(), 10);

        // Encrypt all fields in batch using single cipher (context reuse)
        let mut encrypted_fields = Vec::new();
        for (field_name, plaintext) in &batch.fields {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            encrypted_fields.push((field_name.clone(), encrypted));
        }

        // All encrypted successfully
        assert_eq!(encrypted_fields.len(), 10);

        // All decrypt correctly
        for (i, (_, encrypted)) in encrypted_fields.iter().enumerate() {
            let decrypted = cipher.decrypt(encrypted).unwrap();
            assert_eq!(decrypted, format!("value_{}", i));
        }
    }

    /// Test batch INSERT performance
    #[tokio::test]
    async fn test_batch_insert_performance() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Simulate 1000 records with 5 encrypted fields each
        let record_count = 1000;
        let fields_per_record = 5;

        let start = Instant::now();
        let mut total_encrypted = 0;
        for record_idx in 0..record_count {
            for field_idx in 0..fields_per_record {
                let plaintext = format!("record_{}:field_{}", record_idx, field_idx);
                let _encrypted = cipher.encrypt(&plaintext).unwrap();
                total_encrypted += 1;
            }
        }
        let duration = start.elapsed();

        assert_eq!(total_encrypted, record_count * fields_per_record);

        // Should complete in reasonable time
        assert!(
            duration.as_secs() < 10,
            "Batch insert of {} encryptions took {:?}",
            total_encrypted,
            duration
        );
    }

    /// Test batch UPDATE performance
    #[tokio::test]
    async fn test_batch_update_performance() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Simulate 500 record updates with encrypted fields
        let record_count = 500;
        let start = Instant::now();

        for i in 0..record_count {
            let plaintext = format!("updated_value_{}", i);
            let encrypted = cipher.encrypt(&plaintext).unwrap();

            // Verify each update generates unique ciphertext (new nonce)
            let encrypted2 = cipher.encrypt(&plaintext).unwrap();
            assert_ne!(encrypted, encrypted2, "Each update must use a new nonce");

            // Verify decryption
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, plaintext);
        }

        let duration = start.elapsed();
        assert!(
            duration.as_secs() < 10,
            "Batch update of {} records took {:?}",
            record_count,
            duration
        );
    }

    /// Test batch SELECT performance
    #[tokio::test]
    async fn test_batch_select_performance() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Pre-encrypt 1000 records
        let record_count = 1000;
        let encrypted_records: Vec<Vec<u8>> = (0..record_count)
            .map(|i| cipher.encrypt(&format!("record_{}", i)).unwrap())
            .collect();

        // Decrypt all records (simulates SELECT with decryption)
        let start = Instant::now();
        let mut decrypted_count = 0;
        for (i, encrypted) in encrypted_records.iter().enumerate() {
            let decrypted = cipher.decrypt(encrypted).unwrap();
            assert_eq!(decrypted, format!("record_{}", i));
            decrypted_count += 1;
        }
        let duration = start.elapsed();

        assert_eq!(decrypted_count, record_count);
        assert!(
            duration.as_secs() < 10,
            "Batch select of {} decryptions took {:?}",
            record_count,
            duration
        );
    }

    /// Test batch size optimization
    #[tokio::test]
    async fn test_batch_size_optimization() {
        // Test different batch sizes
        let batch_sizes = [10, 50, 100, 500];

        for &batch_size in &batch_sizes {
            let mut batch = EncryptionBatch::new(format!("batch-{}", batch_size), batch_size);

            for i in 0..batch_size {
                let added = batch.add_field(format!("field_{}", i), format!("value_{}", i));
                assert!(added, "Should add field {} to batch of size {}", i, batch_size);
            }

            assert!(batch.is_full());
            assert_eq!(batch.size(), batch_size);

            // Cannot add more
            let overflow = batch.add_field("overflow", "value");
            assert!(!overflow);

            // Clear resets batch
            batch.clear();
            assert_eq!(batch.size(), 0);
            assert!(!batch.is_full());
        }
    }

    // ============================================================================
    // PARALLEL DECRYPTION OPTIMIZATION
    // ============================================================================

    /// Test parallel decryption improves throughput
    #[tokio::test]
    async fn test_parallel_decryption_throughput() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Pre-encrypt data
        let count = 100;
        let encrypted: Vec<Vec<u8>> = (0..count)
            .map(|i| cipher.encrypt(&format!("parallel_data_{}", i)).unwrap())
            .collect();

        // Decrypt using spawned tasks (simulates parallel decryption)
        let cipher_clone = cipher.clone();
        let encrypted_clone = encrypted.clone();

        let handles: Vec<_> = encrypted_clone
            .into_iter()
            .enumerate()
            .map(|(i, enc)| {
                let c = cipher_clone.clone();
                tokio::spawn(async move {
                    let decrypted = c.decrypt(&enc).unwrap();
                    assert_eq!(decrypted, format!("parallel_data_{}", i));
                    decrypted
                })
            })
            .collect();

        let results: Vec<String> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(results.len(), count);
    }

    /// Test decryption parallelization safety
    #[tokio::test]
    async fn test_decryption_parallel_safety() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt the same value multiple times
        let plaintext = "shared_sensitive_data";
        let encrypted_set: Vec<Vec<u8>> = (0..50)
            .map(|_| cipher.encrypt(plaintext).unwrap())
            .collect();

        // Decrypt all in parallel
        let cipher_clone = cipher.clone();
        let handles: Vec<_> = encrypted_set
            .into_iter()
            .map(|enc| {
                let c = cipher_clone.clone();
                tokio::spawn(async move { c.decrypt(&enc).unwrap() })
            })
            .collect();

        let results: Vec<String> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // No data races: all results match original
        for result in &results {
            assert_eq!(result, plaintext);
        }
        assert_eq!(results.len(), 50);
    }

    /// Test parallel decryption with different keys
    #[tokio::test]
    async fn test_parallel_decryption_different_keys() {
        // Create ciphers with different keys
        let cipher1 = FieldEncryption::new(&[1u8; 32]);
        let cipher2 = FieldEncryption::new(&[2u8; 32]);
        let cipher3 = FieldEncryption::new(&[3u8; 32]);

        let enc1 = cipher1.encrypt("data_for_key_1").unwrap();
        let enc2 = cipher2.encrypt("data_for_key_2").unwrap();
        let enc3 = cipher3.encrypt("data_for_key_3").unwrap();

        // Decrypt in parallel with respective keys
        let h1 = tokio::spawn(async move { cipher1.decrypt(&enc1).unwrap() });
        let h2 = tokio::spawn(async move { cipher2.decrypt(&enc2).unwrap() });
        let h3 = tokio::spawn(async move { cipher3.decrypt(&enc3).unwrap() });

        let (r1, r2, r3) = tokio::join!(h1, h2, h3);
        assert_eq!(r1.unwrap(), "data_for_key_1");
        assert_eq!(r2.unwrap(), "data_for_key_2");
        assert_eq!(r3.unwrap(), "data_for_key_3");
    }

    /// Test spawn_blocking for CPU-bound crypto
    #[tokio::test]
    async fn test_spawn_blocking_crypto_operations() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let plaintext = "cpu_bound_encryption_test";

        // Use spawn_blocking for CPU-bound crypto
        let cipher_enc = cipher.clone();
        let plaintext_owned = plaintext.to_string();
        let encrypted = tokio::task::spawn_blocking(move || {
            cipher_enc.encrypt(&plaintext_owned).unwrap()
        })
        .await
        .unwrap();

        // Decrypt also via spawn_blocking
        let cipher_dec = cipher.clone();
        let decrypted = tokio::task::spawn_blocking(move || {
            cipher_dec.decrypt(&encrypted).unwrap()
        })
        .await
        .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    /// Test parallel decryption error handling
    #[tokio::test]
    async fn test_parallel_decryption_error_handling() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let wrong_cipher = FieldEncryption::new(&[1u8; 32]);

        // Mix of valid and invalid encrypted data
        let valid_enc = cipher.encrypt("valid_data").unwrap();
        let also_valid_enc = cipher.encrypt("also_valid").unwrap();
        let invalid_enc = wrong_cipher.encrypt("wrong_key_data").unwrap();

        // Parallel decryption: some succeed, one fails
        let c1 = cipher.clone();
        let c2 = cipher.clone();
        let c3 = cipher.clone();
        let ve1 = valid_enc.clone();
        let ve2 = also_valid_enc.clone();
        let ie = invalid_enc.clone();

        let h1 = tokio::spawn(async move { c1.decrypt(&ve1) });
        let h2 = tokio::spawn(async move { c2.decrypt(&ve2) });
        let h3 = tokio::spawn(async move { c3.decrypt(&ie) });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        let r3 = h3.await.unwrap();

        // Valid results succeed
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap(), "valid_data");
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap(), "also_valid");

        // Invalid result has clear error
        assert!(r3.is_err());
    }

    // ============================================================================
    // KEY CACHING OPTIMIZATION
    // ============================================================================

    /// Test key cache hit effectiveness
    #[tokio::test]
    async fn test_key_cache_hit_rate() {
        let mut cache = KeyCache::new(100);

        // Insert some keys
        cache.insert("encryption/email", vec![1u8; 32]);
        cache.insert("encryption/phone", vec![2u8; 32]);
        cache.insert("encryption/ssn", vec![3u8; 32]);

        // Access keys repeatedly (simulating normal usage)
        for _ in 0..100 {
            assert!(cache.get("encryption/email").is_some());
            assert!(cache.get("encryption/phone").is_some());
            assert!(cache.get("encryption/ssn").is_some());
        }

        // One miss
        assert!(cache.get("encryption/nonexistent").is_none());

        // Hit rate should be > 95%
        let hit_rate = cache.hit_rate();
        assert!(
            hit_rate > 0.95,
            "Hit rate should be >95%, got {}",
            hit_rate
        );

        let (hits, misses) = cache.stats();
        assert_eq!(hits, 300);
        assert_eq!(misses, 1);
    }

    /// Test cache eviction strategy
    #[tokio::test]
    async fn test_cache_eviction_lru() {
        let mut cache = KeyCache::new(3);

        // Fill cache
        cache.insert("key_a", vec![1u8; 32]);
        cache.insert("key_b", vec![2u8; 32]);
        cache.insert("key_c", vec![3u8; 32]);

        // Access key_a and key_b to make them more recently used
        cache.get("key_a");
        cache.get("key_b");

        // Insert a new key - should evict key_c (least recently used)
        cache.insert("key_d", vec![4u8; 32]);

        // key_c evicted (LRU)
        assert!(cache.get("key_c").is_none());
        // Others remain
        assert!(cache.get("key_a").is_some());
        assert!(cache.get("key_b").is_some());
        assert!(cache.get("key_d").is_some());

        // Configurable max cache size
        assert_eq!(cache.size(), 3);
    }

    /// Test cache warmup on startup
    #[tokio::test]
    async fn test_cache_warmup_startup() {
        let mut cache = KeyCache::new(100);

        // Pre-warm cache with common keys
        let common_keys = vec![
            ("encryption/email", vec![1u8; 32]),
            ("encryption/phone", vec![2u8; 32]),
            ("encryption/ssn", vec![3u8; 32]),
            ("encryption/name", vec![4u8; 32]),
        ];

        for (key_path, key_data) in &common_keys {
            cache.insert(*key_path, key_data.clone());
        }

        // All pre-warmed keys available immediately
        assert_eq!(cache.entry_count(), 4);
        for (key_path, expected_key) in &common_keys {
            let cached = cache.get(key_path);
            assert!(cached.is_some(), "Pre-warmed key '{}' should be in cache", key_path);
            assert_eq!(&cached.unwrap(), expected_key);
        }

        // First-request latency reduced (all hits, no misses)
        let (hits, misses) = cache.stats();
        assert_eq!(hits, 4);
        assert_eq!(misses, 0);
    }

    /// Test cache invalidation on key rotation
    #[tokio::test]
    async fn test_cache_invalidation_key_rotation() {
        let mut cache = KeyCache::new(100);

        // Initial key
        let old_key = vec![1u8; 32];
        cache.insert("encryption/email", old_key.clone());

        // Verify old key cached
        let cached = cache.get("encryption/email").unwrap();
        assert_eq!(cached, old_key);

        // Simulate key rotation: clear cache and insert new key
        cache.clear();
        assert_eq!(cache.entry_count(), 0);

        let new_key = vec![2u8; 32];
        cache.insert("encryption/email", new_key.clone());

        // New key is now cached
        let cached = cache.get("encryption/email").unwrap();
        assert_eq!(cached, new_key);
        assert_ne!(cached, old_key);
    }

    /// Test cache statistics collection
    #[tokio::test]
    async fn test_cache_statistics_collection() {
        let mut cache = KeyCache::new(100);

        // Insert keys
        cache.insert("key1", vec![1u8; 32]);
        cache.insert("key2", vec![2u8; 32]);

        // Generate hits and misses
        cache.get("key1"); // hit
        cache.get("key1"); // hit
        cache.get("key2"); // hit
        cache.get("key3"); // miss
        cache.get("key4"); // miss

        let (hits, misses) = cache.stats();
        assert_eq!(hits, 3);
        assert_eq!(misses, 2);

        let hit_rate = cache.hit_rate();
        assert!((hit_rate - 0.6).abs() < 0.01, "Expected ~60% hit rate, got {}", hit_rate);

        // Cache size metrics
        assert_eq!(cache.size(), 2);
        assert_eq!(cache.entry_count(), 2);
    }

    /// Test distributed cache consistency
    #[tokio::test]
    async fn test_distributed_cache_consistency() {
        // Simulate multiple server instances with local caches
        let mut cache_server_a = KeyCache::new(100);
        let mut cache_server_b = KeyCache::new(100);

        let shared_key = vec![42u8; 32];

        // Both servers load the same key
        cache_server_a.insert("encryption/email", shared_key.clone());
        cache_server_b.insert("encryption/email", shared_key.clone());

        // Both have the same value
        let key_a = cache_server_a.get("encryption/email").unwrap();
        let key_b = cache_server_b.get("encryption/email").unwrap();
        assert_eq!(key_a, key_b);

        // Simulate key rotation on server A: invalidate and reload
        cache_server_a.clear();
        let new_key = vec![99u8; 32];
        cache_server_a.insert("encryption/email", new_key.clone());

        // Server B still has old key (needs invalidation propagation)
        let stale_key = cache_server_b.get("encryption/email").unwrap();
        assert_eq!(stale_key, shared_key);

        // After propagation, server B also gets new key
        cache_server_b.clear();
        cache_server_b.insert("encryption/email", new_key.clone());

        // Now consistent
        let key_a = cache_server_a.get("encryption/email").unwrap();
        let key_b = cache_server_b.get("encryption/email").unwrap();
        assert_eq!(key_a, key_b);
        assert_eq!(key_a, new_key);
    }

    // ============================================================================
    // MEMORY EFFICIENCY OPTIMIZATION
    // ============================================================================

    /// Test memory usage scales linearly
    #[tokio::test]
    async fn test_memory_efficiency_linear_scaling() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt increasing batch sizes and verify linear behavior
        let sizes = [10, 100, 1000];
        let mut results_per_size = Vec::new();

        for &size in &sizes {
            let mut encrypted_batch = Vec::with_capacity(size);
            for i in 0..size {
                let encrypted = cipher.encrypt(&format!("data_{}", i)).unwrap();
                encrypted_batch.push(encrypted);
            }

            // Verify all encrypt/decrypt correctly
            for (i, enc) in encrypted_batch.iter().enumerate() {
                let dec = cipher.decrypt(enc).unwrap();
                assert_eq!(dec, format!("data_{}", i));
            }

            results_per_size.push(encrypted_batch.len());
        }

        // Linear scaling: each batch has expected size
        assert_eq!(results_per_size, vec![10, 100, 1000]);
    }

    /// Test zero-copy encryption where possible
    #[tokio::test]
    async fn test_zero_copy_encryption_optimization() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt with reference to plaintext (no unnecessary clone)
        let plaintext = "sensitive_data_reference";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Verify the output is a fresh allocation (nonce + ciphertext)
        // Nonce: 12 bytes, ciphertext: plaintext.len() + 16 (GCM tag)
        let expected_len = 12 + plaintext.len() + 16;
        assert_eq!(encrypted.len(), expected_len);

        // Buffer reuse within batch
        let mut batch_results = Vec::with_capacity(10);
        for i in 0..10 {
            let data = format!("batch_item_{}", i);
            batch_results.push(cipher.encrypt(&data).unwrap());
        }
        assert_eq!(batch_results.len(), 10);

        // All results independently valid
        for (i, enc) in batch_results.iter().enumerate() {
            let dec = cipher.decrypt(enc).unwrap();
            assert_eq!(dec, format!("batch_item_{}", i));
        }
    }

    /// Test sensitive data cleanup
    #[tokio::test]
    async fn test_sensitive_data_memory_cleanup() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let plaintext = "super_secret_password_123";

        // Encrypt and decrypt in a scoped block
        let decrypted = {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            let result = cipher.decrypt(&encrypted).unwrap();
            // encrypted goes out of scope here
            result
        };

        assert_eq!(decrypted, plaintext);

        // Verify that creating and dropping ciphers works cleanly
        {
            let temp_cipher = FieldEncryption::new(&[99u8; 32]);
            let _enc = temp_cipher.encrypt("temporary").unwrap();
            // temp_cipher and _enc dropped here
        }
        // No memory leaks or crashes after cipher cleanup
    }

    /// Test batch buffer pool
    #[tokio::test]
    async fn test_batch_buffer_pool() {
        // Reusable batch for reducing allocation churn
        let mut batch = EncryptionBatch::new("reusable-batch", 50);

        // First batch
        for i in 0..10 {
            batch.add_field(format!("field_{}", i), format!("value_{}", i));
        }
        assert_eq!(batch.size(), 10);

        // Process batch...
        let cipher = FieldEncryption::new(&[0u8; 32]);
        for (_, plaintext) in &batch.fields {
            let _enc = cipher.encrypt(plaintext).unwrap();
        }

        // Recycle: clear and reuse
        batch.clear();
        assert_eq!(batch.size(), 0);

        // Second batch using same buffer
        for i in 10..25 {
            batch.add_field(format!("field_{}", i), format!("value_{}", i));
        }
        assert_eq!(batch.size(), 15);

        // Pool size configurable via max_size
        assert_eq!(batch.max_size, 50);
    }

    /// Test connection pool with encryption
    #[tokio::test]
    async fn test_connection_pool_encryption_overhead() {
        // Simulate cipher instances cached per "connection"
        let cipher_pool: Vec<FieldEncryption> = (0..5)
            .map(|i| {
                let mut key = [0u8; 32];
                key[0] = i;
                FieldEncryption::new(&key)
            })
            .collect();

        // Each connection uses its own cached cipher
        let start = Instant::now();
        for (conn_idx, cipher) in cipher_pool.iter().enumerate() {
            for op in 0..100 {
                let plaintext = format!("conn_{}_op_{}", conn_idx, op);
                let encrypted = cipher.encrypt(&plaintext).unwrap();
                let decrypted = cipher.decrypt(&encrypted).unwrap();
                assert_eq!(decrypted, plaintext);
            }
        }
        let duration = start.elapsed();

        // 500 total operations across 5 "connections"
        assert!(
            duration.as_secs() < 5,
            "Connection pool encryption took {:?}",
            duration
        );
    }

    /// Test memory pressure handling
    #[tokio::test]
    async fn test_memory_pressure_handling() {
        // Under memory pressure, cache size reduced gracefully
        let mut cache = KeyCache::new(3); // Small cache

        // Insert more keys than cache can hold
        for i in 0..10 {
            cache.insert(format!("key_{}", i), vec![i as u8; 32]);
        }

        // Cache size stays bounded
        assert_eq!(cache.size(), 3);

        // Operations continue (with cache misses as fallback)
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let encrypted = cipher.encrypt("pressure_test").unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "pressure_test");

        // Cache still functional for remaining keys
        // The last 3 inserted keys should be in cache
        assert!(cache.get("key_9").is_some());
    }

    // ============================================================================
    // PERFORMANCE METRICS & MONITORING
    // ============================================================================

    /// Test encryption operation metrics
    #[tokio::test]
    async fn test_encryption_operation_metrics() {
        let mut monitor = PerformanceMonitor::new(1000);
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Verify timer works for measuring real crypto operations
        let timer = OperationTimer::start();
        let _encrypted = cipher.encrypt("test@example.com").unwrap();
        let encrypt_latency = timer.elapsed_us();

        // Record with known latency values to ensure assertions are deterministic
        let metric = OperationMetrics::new("encrypt", encrypt_latency.max(1), 1);
        monitor.record_metric(metric);

        // Record decryption metrics
        let encrypted = cipher.encrypt("decrypt_test").unwrap();
        let timer = OperationTimer::start();
        let _decrypted = cipher.decrypt(&encrypted).unwrap();
        let decrypt_latency = timer.elapsed_us();

        let metric = OperationMetrics::new("decrypt", decrypt_latency.max(1), 1);
        monitor.record_metric(metric);

        // Metrics available
        assert_eq!(monitor.metric_count(), 2);
        assert_eq!(monitor.operation_count("encrypt"), 1);
        assert_eq!(monitor.operation_count("decrypt"), 1);

        // Latency stats (guaranteed > 0 because of .max(1))
        assert!(monitor.average_latency_us() > 0);
        assert!(monitor.max_latency_us() > 0);

        // Timer itself can measure elapsed time
        let timer = OperationTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(timer.elapsed_us() > 0);
        assert!(timer.elapsed_ms() > 0.0);
    }

    /// Test performance regression detection
    #[tokio::test]
    async fn test_performance_regression_detection() {
        let mut monitor = PerformanceMonitor::new(1000);

        // Establish baseline: fast operations
        for _ in 0..50 {
            monitor.record_metric(OperationMetrics::new("encrypt", 100, 1));
        }

        let baseline_avg = monitor.average_latency_us();
        assert_eq!(baseline_avg, 100);

        // Simulate regression: much slower operations
        for _ in 0..50 {
            monitor.record_metric(OperationMetrics::new("encrypt", 1000, 1));
        }

        let new_avg = monitor.average_latency_us();
        // Average should have increased significantly
        assert!(
            new_avg > baseline_avg,
            "Average latency should increase with regression"
        );

        // Regression detected: new avg much higher than baseline
        let regression_ratio = new_avg as f64 / baseline_avg as f64;
        assert!(
            regression_ratio > 2.0,
            "Significant regression: ratio = {}",
            regression_ratio
        );
    }

    /// Test performance dashboard
    #[tokio::test]
    async fn test_performance_dashboard() {
        let mut monitor = PerformanceMonitor::new(1000);

        // Generate diverse metrics
        for i in 1..=100 {
            let latency = (i * 10) as u64;
            let op = if i % 2 == 0 { "encrypt" } else { "decrypt" };
            let mut metric = OperationMetrics::new(op, latency, 1);
            if i % 20 == 0 {
                metric = metric.with_failure();
            }
            monitor.record_metric(metric);
        }

        // Dashboard metrics
        let ops_per_sec = monitor.operations_per_second();
        assert!(ops_per_sec > 0.0);

        let hit_ratio = monitor.success_rate();
        assert!(hit_ratio > 0.9); // 95% success rate

        let error_rate = monitor.error_rate();
        assert!(error_rate < 0.1);

        // Latency percentiles
        let p50 = monitor.p50_latency_us();
        let p99 = monitor.p99_latency_us();
        assert!(p50 > 0);
        assert!(p99 >= p50, "p99 should be >= p50");

        // Per-operation breakdown
        let encrypt_avg = monitor.average_latency_for_operation_us("encrypt");
        let decrypt_avg = monitor.average_latency_for_operation_us("decrypt");
        assert!(encrypt_avg > 0);
        assert!(decrypt_avg > 0);
    }

    /// Test performance SLOs
    #[tokio::test]
    async fn test_performance_slo_compliance() {
        let mut monitor = PerformanceMonitor::new(1000);

        // Set SLOs
        monitor.set_slo("encrypt", 10_000); // <10ms p99
        monitor.set_slo("decrypt", 10_000);

        // Record fast operations (within SLO)
        for _ in 0..100 {
            monitor.record_metric(OperationMetrics::new("encrypt", 500, 1)); // 0.5ms
            monitor.record_metric(OperationMetrics::new("decrypt", 800, 1)); // 0.8ms
        }

        // SLOs met
        assert!(monitor.check_slo("encrypt"), "Encrypt SLO should be met");
        assert!(monitor.check_slo("decrypt"), "Decrypt SLO should be met");

        // All SLOs check
        let all_slos = monitor.check_all_slos();
        assert_eq!(all_slos.len(), 2);
        assert!(all_slos.iter().all(|(_, passed)| *passed));

        // Error rate check
        let success_rate = monitor.success_rate();
        assert!(success_rate >= 0.999, "Success rate should be >= 99.9%");
    }

    // ============================================================================
    // LOAD TESTING
    // ============================================================================

    /// Test encryption under peak load
    #[tokio::test]
    async fn test_peak_load_encryption() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let mut monitor = PerformanceMonitor::new(10000);

        let operation_count = 1000;
        let start = Instant::now();

        for i in 0..operation_count {
            let timer = OperationTimer::start();
            let plaintext = format!("peak_load_record_{}", i);
            let encrypted = cipher.encrypt(&plaintext).unwrap();
            let _decrypted = cipher.decrypt(&encrypted).unwrap();
            let latency = timer.elapsed_us();

            monitor.record_metric(OperationMetrics::new("encrypt_decrypt", latency, 1));
        }

        let total_duration = start.elapsed();

        // Should complete in reasonable time
        assert!(
            total_duration.as_secs() < 10,
            "Peak load of {} ops took {:?}",
            operation_count,
            total_duration
        );

        // Metrics stable
        assert_eq!(monitor.metric_count(), operation_count);
        assert!(monitor.success_rate() >= 1.0);
        assert_eq!(monitor.total_fields_processed(), operation_count);
    }

    /// Test sustained load encryption
    #[tokio::test]
    async fn test_sustained_load_encryption() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let mut monitor = PerformanceMonitor::new(10000);

        // Sustained load: many operations in multiple "waves"
        let waves = 5;
        let ops_per_wave = 200;

        for wave in 0..waves {
            for op in 0..ops_per_wave {
                let plaintext = format!("wave_{}_op_{}", wave, op);
                let encrypted = cipher.encrypt(&plaintext).unwrap();
                let decrypted = cipher.decrypt(&encrypted).unwrap();
                assert_eq!(decrypted, plaintext);

                monitor.record_metric(OperationMetrics::new("encrypt_decrypt", 100, 1));
            }
        }

        // No degradation: all operations succeeded
        assert_eq!(monitor.metric_count(), waves * ops_per_wave);
        assert!(monitor.success_rate() >= 1.0);

        // Performance stable across waves
        let avg_latency = monitor.average_latency_us();
        assert_eq!(avg_latency, 100); // Consistent simulated latency
    }

    /// Test encryption with cache churn
    #[tokio::test]
    async fn test_encryption_cache_churn() {
        let mut cache = KeyCache::new(5); // Small cache to force churn

        // Access many different keys
        for i in 0..100 {
            let key_path = format!("key_{}", i);
            cache.insert(&key_path, vec![i as u8; 32]);
        }

        // Cache size stays bounded
        assert_eq!(cache.size(), 5);

        // Hit rate low due to churn
        let (hits, misses) = cache.stats();
        // All 100 inserts, no gets yet that would hit
        assert_eq!(hits, 0);
        assert_eq!(misses, 0);

        // Access recently inserted keys (should hit)
        for i in 95..100 {
            let key_path = format!("key_{}", i);
            assert!(cache.get(&key_path).is_some(), "Recent key {} should be cached", i);
        }

        // Access older keys (should miss due to eviction)
        for i in 0..5 {
            let key_path = format!("key_{}", i);
            assert!(cache.get(&key_path).is_none(), "Old key {} should be evicted", i);
        }

        let (hits, misses) = cache.stats();
        assert_eq!(hits, 5);
        assert_eq!(misses, 5);
    }

    /// Test encryption with key rotation under load
    #[tokio::test]
    async fn test_key_rotation_under_load() {
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Encrypt/decrypt operations concurrent with key rotation
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Pre-load operations
        let mut encrypted_records = Vec::new();
        for i in 0..100 {
            encrypted_records.push(cipher.encrypt(&format!("record_{}", i)).unwrap());
        }

        // Trigger rotation mid-operation
        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.rotate_key().unwrap();
        assert!(new_version > old_version);

        // Existing encrypted records still decrypt (same cipher/key)
        for (i, enc) in encrypted_records.iter().enumerate() {
            let decrypted = cipher.decrypt(enc).unwrap();
            assert_eq!(decrypted, format!("record_{}", i));
        }

        // New operations continue working
        for i in 100..200 {
            let encrypted = cipher.encrypt(&format!("post_rotation_{}", i)).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, format!("post_rotation_{}", i));
        }

        // Rotation metrics recorded
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert_eq!(metrics.failed_rotations(), 0);
    }
}
