# Test ownership

`fixtures/intents/`: partial/resolved intent examples.
`fixtures/catalog/`: valid and invalid generic archetype examples.
`fixtures/scenarios/`: topology, policy, load and failure scenarios.
`integration/`: hosted API/worker/storage/client acceptance.
`reference/`: independent reference models for algorithm checks.

Intent, invalid catalog and parallel-link fixtures now exist and are exercised by
engine/tests/. Root tests are not automatically discovered by Cargo; hosted integration
and independent algorithm reference harnesses remain future work. Run
`cargo test -p labyrinth-engine` for current behavior and schema checks.
