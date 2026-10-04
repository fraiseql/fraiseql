//! `AdminPrincipal` scoping rules (#1089).

use super::*;

const T: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);
const U: Uuid = Uuid::from_u128(0x2222_2222_2222_4222_8222_2222_2222_2222);

#[test]
fn the_platform_gets_the_tenant_it_names_or_every_tenant() {
    assert_eq!(AdminPrincipal::Platform.scope(None), Ok(None));
    assert_eq!(AdminPrincipal::Platform.scope(Some(U)), Ok(Some(U)));
}

#[test]
fn a_tenant_principal_is_confined_to_its_own_tenant() {
    let t = AdminPrincipal::Tenant(T);
    assert_eq!(t.scope(None), Ok(Some(T)), "naming no tenant means its own, never all");
    assert_eq!(t.scope(Some(T)), Ok(Some(T)));
    assert_eq!(t.scope(Some(U)), Err(ForeignTenant), "another tenant is refused");
}

#[test]
fn a_tenant_principal_sees_only_its_own_rows() {
    let t = AdminPrincipal::Tenant(T);
    assert!(t.may_see(Some(T)));
    assert!(!t.may_see(Some(U)), "another tenant's row");
    assert!(!t.may_see(None), "a platform row");
    assert!(AdminPrincipal::Platform.may_see(None));
    assert!(AdminPrincipal::Platform.may_see(Some(U)));
}
