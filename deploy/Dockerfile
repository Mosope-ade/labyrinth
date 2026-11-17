# Base identities verified against Docker Hub's official library tag metadata.
FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock LICENSE ./
COPY engine ./engine
COPY api ./api
COPY worker ./worker
COPY cli ./cli
COPY service-auth ./service-auth
COPY tests/fixtures ./tests/fixtures
RUN cargo build --release --locked -p labyrinth-api -p labyrinth-worker --bins \
    && cargo build --release --locked -p labyrinth-api --example migrate --example prune_audit \
    && cargo build --release --locked -p labyrinth-worker --example container_fixture \
    && /src/target/release/examples/container_fixture > /tmp/container-fixture.json

# Export synthetic smoke input without shipping it in the production runtime.
FROM scratch AS fixture
COPY --from=build /tmp/container-fixture.json /container-fixture.json

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgcc-s1 util-linux \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY LICENSE /app/LICENSE
COPY --from=build /src/target/release/labyrinth-api /app/labyrinth-api
COPY --from=build /src/target/release/labyrinth-worker /app/labyrinth-worker
COPY --from=build /src/target/release/examples/migrate /app/labyrinth-migrate
COPY --from=build /src/target/release/examples/prune_audit /app/labyrinth-prune-audit
USER 65532:65532
CMD ["/app/labyrinth-api"]
