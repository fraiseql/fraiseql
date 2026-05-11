use super::*;

#[test]
fn test_factory_creation() {
    let factory = WireClientFactory::new("postgres://localhost/test");
    assert_eq!(factory.connection_string(), "postgres://localhost/test");
}

#[test]
fn test_factory_clone() {
    let factory1 = WireClientFactory::new("postgres://localhost/test");
    let factory2 = factory1.clone();
    assert_eq!(factory1.connection_string(), factory2.connection_string());
}
