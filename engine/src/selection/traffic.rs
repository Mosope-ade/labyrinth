use crate::{
    model::*,
    selection::{self, SelectionError},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBatch {
    pub cohort: u16,
    pub session_peak_mbps: u64,
    pub count: u64,
}
/// Grow exactly one declared dimension. Integer session granularity is recorded
/// separately from ceil(aggregate peak * growth), never hidden as a second growth.
pub fn batches(demand: &TrafficDemand, growth: u8) -> Result<Vec<SessionBatch>, SelectionError> {
    match &demand.distribution {
        Some(TrafficDistribution::SinglePath) => Ok(vec![SessionBatch {
            cohort: 0,
            session_peak_mbps: selection::with_growth(demand.peak_mbps, growth)?,
            count: 1,
        }]),
        Some(TrafficDistribution::Sessions {
            cohorts,
            growth: kind,
        }) => cohorts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let (rate, count) = match kind {
                    Some(SessionGrowth::Concurrency) => (
                        c.session_peak_mbps,
                        selection::with_growth(u64::from(c.simultaneous_sessions), growth)?,
                    ),
                    _ => (
                        selection::with_growth(c.session_peak_mbps, growth)?,
                        u64::from(c.simultaneous_sessions),
                    ),
                };
                Ok(SessionBatch {
                    cohort: u16::try_from(i).expect("bounded cohorts"),
                    session_peak_mbps: rate,
                    count,
                })
            })
            .collect(),
        None => Err(SelectionError::ClarificationRequired(
            "traffic distribution is required".into(),
        )),
    }
}
pub fn reserved_peak(demand: &TrafficDemand, growth: u8) -> Result<u64, SelectionError> {
    let total = batches(demand, growth)?
        .iter()
        .map(|c| u128::from(c.session_peak_mbps) * u128::from(c.count))
        .sum::<u128>();
    u64::try_from(total).map_err(|_| SelectionError::ArithmeticOverflow)
}
