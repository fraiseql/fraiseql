use super::*;

#[test]
fn test_conservative_estimator() {
    let est = ConservativeEstimator;
    assert_eq!(est.estimate_bytes(0), 0);
    assert_eq!(est.estimate_bytes(1), 2048);
    assert_eq!(est.estimate_bytes(100), 204_800);
    assert_eq!(est.estimate_bytes(256), 524_288);
}

#[test]
fn test_conservative_name() {
    let est = ConservativeEstimator;
    assert_eq!(est.name(), "conservative_2kb");
}

#[test]
fn test_fixed_estimator() {
    let est = FixedEstimator::new(4096);
    assert_eq!(est.estimate_bytes(0), 0);
    assert_eq!(est.estimate_bytes(1), 4096);
    assert_eq!(est.estimate_bytes(100), 409_600);
    assert_eq!(est.estimate_bytes(256), 1_048_576);
}

#[test]
fn test_fixed_estimator_custom_sizes() {
    for size in &[1024, 2048, 4096, 8192] {
        let est = FixedEstimator::new(*size);
        assert_eq!(est.estimate_bytes(10), 10 * size);
    }
}

#[test]
fn test_fixed_estimator_overflow_safe() {
    let est = FixedEstimator::new(usize::MAX / 2);
    // Should not panic on overflow
    let _ = est.estimate_bytes(usize::MAX);
}
