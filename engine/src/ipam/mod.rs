//! Topology-independent, deterministic IPv4 and VLAN allocation.
mod allocate;
mod types;
mod vlan;
pub use allocate::allocate;
pub use types::*;
pub use vlan::allocate_vlans;
