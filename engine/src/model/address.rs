use ipnet::Ipv4Net;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Canonical IPv4 network CIDR; host bits are rejected, never silently discarded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Ipv4Prefix(String);
impl Ipv4Prefix {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let net = value
            .parse::<Ipv4Net>()
            .map_err(|_| "expected IPv4 network CIDR")?;
        if net.addr() != net.network() || net.to_string() != value {
            return Err("CIDR must be canonical with zero host bits".into());
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn contains(&self, other: &Self) -> bool {
        self.net().contains(&other.net())
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
    pub(crate) fn net(&self) -> Ipv4Net {
        self.0.parse().expect("validated prefix invariant")
    }
}
impl<'de> Deserialize<'de> for Ipv4Prefix {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Host address with prefix. /31 and /32 retain their point-to-point/host semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Ipv4InterfaceAddress(String);
impl Ipv4InterfaceAddress {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let net = value
            .parse::<Ipv4Net>()
            .map_err(|_| "expected IPv4 interface CIDR")?;
        if net.to_string() != value
            || (net.prefix_len() < 31
                && (net.addr() == net.network() || net.addr() == net.broadcast()))
        {
            return Err("expected canonical usable host address, not network/broadcast".into());
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn net(&self) -> Ipv4Net {
        self.0
            .parse()
            .expect("validated interface-address invariant")
    }
}
impl<'de> Deserialize<'de> for Ipv4InterfaceAddress {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
