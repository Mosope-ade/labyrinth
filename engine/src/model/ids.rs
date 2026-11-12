//! Validated stable identifiers. IDs do not encode presentation labels.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(#[schemars(regex(pattern = "^[a-z][a-z0-9_-]{0,63}$"))] String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.len() > 64 || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                    || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
                    return Err("expected 1-64 lowercase ASCII letters/digits/hyphens/underscores, starting with a letter".into());
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
        }
    };
}
identifier!(SiteId);
identifier!(ZoneId);
identifier!(DeviceId);
identifier!(InterfaceId);
identifier!(LinkId);
identifier!(ArchetypeId);
identifier!(ModeId);
identifier!(PortGroupId);
identifier!(FailureDomainId);
identifier!(FlowId);
identifier!(RevisionId);
identifier!(DomainId);
identifier!(PolicyId);
identifier!(BundleId);

identifier!(AllocationId);
identifier!(DecisionId);
