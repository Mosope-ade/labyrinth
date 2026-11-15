# Five-pass verification contract

A `VerifiedPackage` requires all five passes. Failure, unsupported assumptions,
search limits and insufficient proof budgets cannot be relabeled as success.

## Structural

Check canonical references, exact catalog identity, selected archetype/mode capabilities,
physical port uniqueness/count/speed/media, cable/bundle compatibility, L2 tree coverage
and loop freedom, gateway/VIP ownership, and agreement with immutable generation context.
The combined physical/explicit-WAN graph must connect the design. Baseline capacity
checks reject over-subscribed ports, shared groups, operating modes and WAN services.

## Addressing

Check canonical CIDRs, sibling overlap, host uniqueness/capacity, gateway reservation
ranges, pool containment, exclusions, and agreement with allocated/pinned addresses.
VLAN uniqueness uses the configured site/global scope. Parent pool containment is not
sibling overlap. Gateway reservations and member/VIP addresses are explicit.

## Routing

Validate selected next-hop adjacency and walk destination tables independently to
reject missing routes, loops and wrong terminals. Every surviving zone gateway must
have a backbone path to other surviving zone gateways even when policy denies traffic.
Firewalls terminate zones and cannot be transit shortcuts for unrelated zones.

Session-path verification independently conserves every cohort count, checks grown
per-session rates and aggregate reservations, active source/destination owners,
loop freedom, live adjacent transports, and steering capability at every routing hop.
Single-path reservations must exactly follow the destination table. Sessions remain
indivisible; an aggregate using multiple paths never proves one session can exceed a
member rate. Controlled steering is an explicit implementation requirement, not a
prediction of ordinary ECMP/LACP hash balance.

## Security

Stateless, directional deny-overrides and default deny apply at source gateway and
all destination firewall data ingresses, including standby members. Actual bindings
must exactly match the requested rules and enforcement locations. Missing rules,
unexpected rules and conflicting definitions reject. An all-deny design cannot satisfy
requested allowed traffic. Return traffic needs its own permit and demand declaration.

Partition each zone pair's TCP/UDP port space at every relevant rule boundary, and
cover all other IP protocol numbers. One representative per equivalence class is an
exhaustive proof under these predicates, not port sampling. A rule-free pair is proved
default-deny symbolically across all classes. Proof covers the bounded IPv4 service
model (TCP/UDP ports 1–65535); it does not model malformed headers, stateful sessions,
IPv6, NAT transformation or implementation-specific protocol behavior.

## Resilience and capacity

Recompute L2 state, active/standby ownership, routes and session placement for each
requested failure. Use the last committed owner, preserve no-preemption semantics,
permit coordinated local path-health failover only with live HA control, and fail closed
when both-live peers lose every control path.

- Tier0: no failure guarantee; the report says so explicitly.
- Tier1: every relevant internal cable, logical WAN transport and physical WAN handoff.
- Tier2: Tier1 plus every core/distribution/firewall/WAN-edge device.
- Tier3: Tier2 plus every declared site and provider failure domain.

Single-homed endpoint ports and access-switch device loss are outside backbone device
protection. Failed-site endpoint demands are explicitly excluded; surviving site/zone
backbone paths still must work. Two geographic hubs/provider declarations are checked
before Tier3 generation. Explicit transport declarations are assumptions about carrier
service, not a proof of actual physical provider internals or route diversity.

Resource accounting uses simultaneous declared traffic, with growth applied once.
Session growth explicitly changes rates or concurrency; integer cohort granularity is
reported separately. Directional physical-port limits and WAN attachment budgets are
checked without multiplying service capacity by handoff count. Shared port-group limits
count aggregate transmit/receive I/O. Mode throughput counts forwarded load, including
a conservative full-zone HA state-copy reservation on one surviving control channel.
Firewall selection budgets data plus that copy. Unknown mode throughput cannot pass.
A null shared-group limit asserts no *additional* group bottleneck beyond the mode
limit; catalog authors must not encode an unknown bottleneck as an absent restriction.

Unknown endpoint placement is bounded conservatively across each active zone tree:
traffic may concentrate in any surviving access population, limited by its endpoint
line rates. LACP members are not summed to justify individual-flow capacity. The
verifier checks robust per-member envelopes and group budgets without counting all
alternative member envelopes as simultaneous duplicate traffic.

Every pass has checked counts and bounded counterexamples. Failure scenarios record
protected/excluded demand counts, concrete removed components and capacity/reachability
bottlenecks. Default budgets are 4096 scenarios and 1,000,000 service classes per
scenario. Exhaustion is an explicit failure; no untested remainder is marked passed.
