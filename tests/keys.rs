//! Integration tests for key extractors.

use skp_ratelimit::key::{CompositeKey, CompositeKey3, EitherKey, Key, OptionalKey, StaticKey};

#[test]
fn test_composite_key_two_parts() {
    let key = CompositeKey::new(StaticKey::new("user:123"), StaticKey::new("/api/data"));

    let result = key.extract(&());
    assert_eq!(result, Some("user:123:/api/data".to_string()));
}

#[test]
fn test_composite_key_with_separator() {
    let key = CompositeKey::with_separator(StaticKey::new("a"), StaticKey::new("b"), "|");

    let result = key.extract(&());
    assert_eq!(result, Some("a|b".to_string()));
}

#[test]
fn test_composite_key3() {
    let key = CompositeKey3::new(
        StaticKey::new("ip:1.2.3.4"),
        StaticKey::new("user:admin"),
        StaticKey::new("path:/api"),
    );

    let result = key.extract(&());
    assert_eq!(result, Some("ip:1.2.3.4:user:admin:path:/api".to_string()));
}

#[test]
fn test_either_key_uses_primary() {
    let key = EitherKey::new(StaticKey::new("primary"), StaticKey::new("fallback"));

    let result = key.extract(&());
    assert_eq!(result, Some("primary".to_string()));
}

#[test]
fn test_optional_key_with_value() {
    let key = OptionalKey::new(StaticKey::new("value"), "default");

    let result = key.extract(&());
    assert_eq!(result, Some("value".to_string()));
}
