//! Physical binding and addressing stage. Logical forwarding is deliberately separate.
mod build;
mod types;
pub(crate) use build::build;
pub use types::*;
mod forwarding;
pub use crate::model::Layer2Forwarding;
pub use crate::model::{Failures, OwnershipState, OwnershipStatus};
pub use forwarding::l2_forwarding;
mod ha;
pub use ha::{LogicalPolicy, ha_ownership};
mod assemble;
pub use assemble::assemble;

pub fn effective_failures(t: &AllocatedTopology, s: &crate::model::FailureScenario) -> Failures {
    let mut out = s.internal.clone();
    for link in t.links() {
        if s.interfaces.contains(&link.from.interface) || s.interfaces.contains(&link.to.interface)
        {
            out.links.insert(link.id.clone());
        }
    }
    for p in &t.selection().blueprint.placements {
        if s.sites.contains(&p.site) {
            out.devices.insert(p.device.clone());
        }
    }
    out
}
