# aurelius-mcp over Streamable HTTP: MCP at http://<host>:7464/mcp, reaching
# the Aurelius server through its remote gateway (AURELIUS_MCP_URL).
# See docs/aurelius-mcp.md, "Docker".
#
#   docker build -t aurelius-mcp .

# ---- build ----
FROM rust:1.99-bookworm AS build
# aws-lc-rs (rustls) may build its C part with cmake.
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# Cache mounts keep the registry and the build folder between builds (BuildKit).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p aurelius-mcp --bin aurelius-mcp \
    && cp target/release/aurelius-mcp /usr/local/bin/aurelius-mcp

# ---- run ----
FROM debian:bookworm-slim
# CA certificates: TLS to the gateway's ts.net name (a Let's Encrypt certificate).
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home aurelius
COPY --from=build /usr/local/bin/aurelius-mcp /usr/local/bin/aurelius-mcp
USER aurelius
EXPOSE 7464
ENTRYPOINT ["aurelius-mcp"]
CMD ["--http", "0.0.0.0:7464"]
