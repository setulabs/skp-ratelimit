//! Composite key for combining multiple extractors.

use crate::key::Key;

/// Combine two key extractors into a composite key.
///
/// The resulting key is formatted as `"{key1}:{key2}"`.
///
/// # Example
///
/// ```ignore
/// use oc_ratelimit_advanced::key::{CompositeKey, IpKey, PathKey};
///
/// // Rate limit by IP + path
/// let key = CompositeKey::new(IpKey::new(), PathKey::new());
/// // Results in keys like "192.168.1.1:/api/users"
/// ```
#[derive(Debug, Clone)]
pub struct CompositeKey<K1, K2> {
    first: K1,
    second: K2,
    separator: &'static str,
}

impl<K1, K2> CompositeKey<K1, K2> {
    /// Create a new composite key with default separator `:`.
    pub fn new(first: K1, second: K2) -> Self {
        Self {
            first,
            second,
            separator: ":",
        }
    }

    /// Create a new composite key with custom separator.
    pub fn with_separator(first: K1, second: K2, separator: &'static str) -> Self {
        Self {
            first,
            second,
            separator,
        }
    }
}

impl<R, K1, K2> Key<R> for CompositeKey<K1, K2>
where
    K1: Key<R>,
    K2: Key<R>,
{
    fn extract(&self, request: &R) -> Option<String> {
        let k1 = self.first.extract(request)?;
        let k2 = self.second.extract(request)?;
        Some(format!("{}{}{}", k1, self.separator, k2))
    }

    fn name(&self) -> &'static str {
        "composite"
    }
}

/// Combine three key extractors.
#[derive(Debug, Clone)]
pub struct CompositeKey3<K1, K2, K3> {
    first: K1,
    second: K2,
    third: K3,
    separator: &'static str,
}

impl<K1, K2, K3> CompositeKey3<K1, K2, K3> {
    /// Create a new 3-part composite key.
    pub fn new(first: K1, second: K2, third: K3) -> Self {
        Self {
            first,
            second,
            third,
            separator: ":",
        }
    }
}

impl<R, K1, K2, K3> Key<R> for CompositeKey3<K1, K2, K3>
where
    K1: Key<R>,
    K2: Key<R>,
    K3: Key<R>,
{
    fn extract(&self, request: &R) -> Option<String> {
        let k1 = self.first.extract(request)?;
        let k2 = self.second.extract(request)?;
        let k3 = self.third.extract(request)?;
        Some(format!(
            "{}{}{}{}{}",
            k1, self.separator, k2, self.separator, k3
        ))
    }

    fn name(&self) -> &'static str {
        "composite3"
    }
}

/// Either key - use first if available, otherwise second.
#[derive(Debug, Clone)]
pub struct EitherKey<K1, K2> {
    primary: K1,
    fallback: K2,
}

impl<K1, K2> EitherKey<K1, K2> {
    /// Create a new either key.
    ///
    /// Uses primary if it extracts successfully, otherwise falls back to secondary.
    pub fn new(primary: K1, fallback: K2) -> Self {
        Self { primary, fallback }
    }
}

impl<R, K1, K2> Key<R> for EitherKey<K1, K2>
where
    K1: Key<R>,
    K2: Key<R>,
{
    fn extract(&self, request: &R) -> Option<String> {
        self.primary
            .extract(request)
            .or_else(|| self.fallback.extract(request))
    }

    fn name(&self) -> &'static str {
        "either"
    }
}

/// Optional key wrapper - always succeeds, uses default if extraction fails.
#[derive(Debug, Clone)]
pub struct OptionalKey<K> {
    inner: K,
    default: String,
}

impl<K> OptionalKey<K> {
    /// Create a new optional key with a default value.
    pub fn new(inner: K, default: impl Into<String>) -> Self {
        Self {
            inner,
            default: default.into(),
        }
    }
}

impl<R, K> Key<R> for OptionalKey<K>
where
    K: Key<R>,
{
    fn extract(&self, request: &R) -> Option<String> {
        Some(
            self.inner
                .extract(request)
                .unwrap_or_else(|| self.default.clone()),
        )
    }

    fn name(&self) -> &'static str {
        "optional"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::StaticKey;

    #[test]
    fn test_composite_key() {
        let key = CompositeKey::new(StaticKey::new("ip"), StaticKey::new("path"));
        assert_eq!(key.extract(&()), Some("ip:path".to_string()));
    }

    #[test]
    fn test_composite_key3() {
        let key = CompositeKey3::new(
            StaticKey::new("a"),
            StaticKey::new("b"),
            StaticKey::new("c"),
        );
        assert_eq!(key.extract(&()), Some("a:b:c".to_string()));
    }

    #[test]
    fn test_either_key_primary() {
        let key = EitherKey::new(StaticKey::new("primary"), StaticKey::new("fallback"));
        assert_eq!(key.extract(&()), Some("primary".to_string()));
    }

    #[test]
    fn test_optional_key() {
        let key = OptionalKey::new(StaticKey::new("value"), "default");
        assert_eq!(key.extract(&()), Some("value".to_string()));
    }
}
