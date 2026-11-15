use super::*;
use std::time::{Duration, Instant};
#[derive(Clone)]
struct Candidate {
    devices: Vec<DeviceId>,
    hops: Vec<ForwardingAdjacency>,
}
struct Work {
    flow: FlowId,
    from: ZoneId,
    to: ZoneId,
    batch: SessionBatch,
    steered: bool,
    paths: Vec<Candidate>,
}
struct Budget {
    states: u64,
    limit: u64,
    deadline: Instant,
    truncated: bool,
}
impl Budget {
    fn tick(&mut self) -> Result<(), RoutingError> {
        if Instant::now() >= self.deadline {
            return Err(RoutingError::Timeout);
        }
        if self.states >= self.limit {
            return Err(RoutingError::SearchExhausted {
                states: self.states,
                detail:
                    "session path/allocation search budget exhausted; no global infeasibility proof"
                        .into(),
            });
        }
        self.states += 1;
        Ok(())
    }
}
fn candidates(
    from: &DeviceId,
    to: &DeviceId,
    adjacent: &BTreeMap<DeviceId, Vec<(DeviceId, ForwardingAdjacency)>>,
    firewalls: &BTreeSet<DeviceId>,
    max: usize,
    budget: &mut Budget,
) -> Result<Vec<Candidate>, RoutingError> {
    let mut queue = VecDeque::from([Candidate {
        devices: vec![from.clone()],
        hops: vec![],
    }]);
    let mut found = Vec::new();
    while let Some(path) = queue.pop_front() {
        budget.tick()?;
        let current = path.devices.last().expect("nonempty path");
        if current == to {
            found.push(path);
            if found.len() == max {
                budget.truncated |= !queue.is_empty();
                break;
            }
            continue;
        }
        for (peer, via) in adjacent.get(current).into_iter().flatten() {
            if path.devices.contains(peer) || (peer != to && firewalls.contains(peer)) {
                continue;
            }
            if queue.len() >= 4096 {
                budget.truncated = true;
                continue;
            }
            let mut next = path.clone();
            next.devices.push(peer.clone());
            next.hops.push(via.clone());
            queue.push_back(next);
        }
    }
    Ok(found)
}
fn allocation(w: &Work, path: &Candidate, count: u64) -> FlowPath {
    FlowPath {
        flow: w.flow.clone(),
        from_zone: w.from.clone(),
        to_zone: w.to.clone(),
        cohort: w.batch.cohort,
        sessions: count,
        session_peak_mbps: w.batch.session_peak_mbps,
        steered: w.steered,
        peak_mbps: u64::try_from(u128::from(count) * u128::from(w.batch.session_peak_mbps))
            .expect("bounded cohort peak"),
        devices: path.devices.clone(),
        adjacencies: path.hops.clone(),
    }
}
struct Search<'a> {
    intent: &'a ResolvedIntent,
    t: &'a AllocatedTopology,
    wan: &'a [WanConnection],
    scenario: &'a FailureScenario,
    work: &'a [Work],
    budget: Budget,
    snapshot: RoutingSnapshot,
    last_capacity_errors: Vec<crate::diagnostics::Counterexample>,
}
impl Search<'_> {
    fn fits(&mut self, path: FlowPath) -> Result<bool, RoutingError> {
        self.budget.tick()?;
        self.snapshot.paths.push(path);
        let errors = crate::capacity::check(
            self.intent,
            self.t.catalog(),
            self.t,
            self.wan,
            &self.snapshot,
            self.scenario,
        );
        let good = errors.is_empty();
        if !good {
            self.last_capacity_errors = errors;
        }
        self.snapshot.paths.pop();
        Ok(good)
    }
    fn solve(
        &mut self,
        index: usize,
        path_index: usize,
        remaining: u64,
        depth: usize,
    ) -> Result<bool, RoutingError> {
        self.budget.tick()?;
        if depth > 512 {
            return Err(RoutingError::SearchExhausted {
                states: self.budget.states,
                detail: "session allocation depth bound exceeded".into(),
            });
        }
        if index == self.work.len() {
            return Ok(true);
        }
        if remaining == 0 {
            let next = index + 1;
            return self.solve(
                next,
                0,
                self.work.get(next).map_or(0, |w| w.batch.count),
                depth + 1,
            );
        }
        let works = self.work;
        let w = &works[index];
        let Some(path) = w.paths.get(path_index) else {
            return Ok(false);
        };
        let mut lo = 0;
        let mut hi = remaining;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.fits(allocation(w, path, mid))? {
                lo = mid
            } else {
                hi = mid - 1
            }
        }
        let mut count = lo;
        loop {
            if count > 0 {
                self.snapshot.paths.push(allocation(w, path, count));
            }
            let solved = if count == remaining {
                self.solve(
                    index + 1,
                    0,
                    self.work.get(index + 1).map_or(0, |w| w.batch.count),
                    depth + 1,
                )?
            } else {
                self.solve(index, path_index + 1, remaining - count, depth + 1)?
            };
            if solved {
                return Ok(true);
            }
            if count > 0 {
                self.snapshot.paths.pop();
            } else {
                break;
            }
            count -= 1;
        }
        Ok(false)
    }
}
pub(super) fn pack(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    logical: &LogicalPolicy,
    scenario: &FailureScenario,
    mut snapshot: RoutingSnapshot,
    adjacent: &BTreeMap<DeviceId, Vec<(DeviceId, ForwardingAdjacency)>>,
) -> Result<RoutingSnapshot, RoutingError> {
    let limits = &logical.routing_limits;
    if limits.max_search_states == 0
        || limits.max_search_states > 1_000_000
        || limits.max_paths_per_demand == 0
        || limits.max_paths_per_demand > 256
        || limits.timeout_ms == 0
        || limits.timeout_ms > 15_000
    {
        return Err(RoutingError::InvalidLimits);
    }
    let mut budget = Budget {
        states: 0,
        limit: limits.max_search_states,
        deadline: Instant::now() + Duration::from_millis(limits.timeout_ms),
        truncated: false,
    };
    let firewalls = t
        .selection()
        .assignment
        .devices
        .iter()
        .filter(|d| d.role == DeviceRole::Firewall)
        .map(|d| d.requirement.clone())
        .collect();
    let mut work = Vec::new();
    for demand in &intent.data().demands {
        let Some(existing) = snapshot.paths.iter().find(|p| p.flow == demand.id) else {
            continue;
        }; // endpoint in failed site
        let steered = matches!(
            demand.distribution,
            Some(TrafficDistribution::Sessions { .. })
        );
        let paths = if steered {
            candidates(
                existing.devices.first().expect("source"),
                existing.devices.last().expect("target"),
                adjacent,
                &firewalls,
                limits.max_paths_per_demand,
                &mut budget,
            )?
        } else {
            vec![Candidate {
                devices: existing.devices.clone(),
                hops: existing.adjacencies.clone(),
            }]
        };
        if paths.is_empty() {
            return Err(RoutingError::Unreachable {
                from: demand.from_zone.clone(),
                to: demand.to_zone.clone(),
            });
        }
        for batch in batches(demand, intent.data().growth_percent).map_err(PlanningError::from)? {
            work.push(Work {
                flow: demand.id.clone(),
                from: demand.from_zone.clone(),
                to: demand.to_zone.clone(),
                batch,
                steered,
                paths: paths.clone(),
            });
            if work.len() > 512 {
                return Err(RoutingError::SearchExhausted {
                    states: budget.states,
                    detail: "at most 512 session cohorts per routing job".into(),
                });
            }
        }
    }
    work.sort_by(|a, b| {
        b.batch
            .session_peak_mbps
            .cmp(&a.batch.session_peak_mbps)
            .then((&a.flow, a.batch.cohort).cmp(&(&b.flow, b.batch.cohort)))
    });
    snapshot.paths.clear();
    let count = work.first().map_or(0, |w| w.batch.count);
    let mut search = Search {
        intent,
        t,
        wan,
        scenario,
        work: &work,
        budget,
        snapshot,
        last_capacity_errors: vec![],
    };
    if search.solve(0, 0, count, 0)? {
        search.snapshot.paths.sort_by(|a, b| {
            (&a.flow, a.cohort, &a.devices, &a.adjacencies).cmp(&(
                &b.flow,
                b.cohort,
                &b.devices,
                &b.adjacencies,
            ))
        });
        Ok(search.snapshot)
    } else if !search.budget.truncated {
        Err(RoutingError::UnsatisfiedTraffic {
            flow: work.first().expect("nonempty infeasible work").flow.clone(),
            bottlenecks: search.last_capacity_errors,
        })
    } else {
        Err(RoutingError::SearchExhausted{states:search.budget.states,detail:"no feasible session assignment in the truncated candidate path set; no infeasibility proof".into()})
    }
}
