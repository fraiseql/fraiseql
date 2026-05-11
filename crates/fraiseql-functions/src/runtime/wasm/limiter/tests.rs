use super::*;

#[test]
fn test_limiter_tracks_peak_memory() {
    let mut limiter = FunctionStoreLimiter::new(1024 * 1024);  // 1MB limit

    // Simulate memory growth
    assert!(limiter.memory_growing(0, 512 * 1024, None).is_ok());
    assert_eq!(limiter.stats().current_memory, 512 * 1024);
    assert_eq!(limiter.stats().peak_memory, 512 * 1024);

    // Grow more
    assert!(limiter.memory_growing(512 * 1024, 800 * 1024, None).is_ok());
    assert_eq!(limiter.stats().current_memory, 800 * 1024);
    assert_eq!(limiter.stats().peak_memory, 800 * 1024);

    // Shrink (shouldn't affect peak)
    assert!(limiter.memory_growing(800 * 1024, 600 * 1024, None).is_ok());
    assert_eq!(limiter.stats().current_memory, 600 * 1024);
    assert_eq!(limiter.stats().peak_memory, 800 * 1024);
}

#[test]
fn test_limiter_enforces_memory_limit() {
    let mut limiter = FunctionStoreLimiter::new(1024 * 1024);  // 1MB limit

    // Growing within limit succeeds
    assert!(limiter.memory_growing(0, 512 * 1024, None).is_ok());

    // Growing beyond limit fails
    assert!(limiter.memory_growing(512 * 1024, 2 * 1024 * 1024, None).is_err());

    // Current memory should still be at the exceeded value (checked during growth)
    assert_eq!(limiter.stats().current_memory, 2 * 1024 * 1024);
}

#[test]
fn test_limiter_allows_table_growth() {
    let mut limiter = FunctionStoreLimiter::new(1024 * 1024);

    // Table growth is always allowed
    assert!(limiter.table_growing(0_usize, 100_usize, None).is_ok());
    assert!(limiter.table_growing(100_usize, 1000_usize, None).is_ok());
}
