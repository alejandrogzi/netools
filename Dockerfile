# Minimal container exposing the `netools` CLI.
#
# Build from the repository root (the directory containing the `netools/` crate):
#   docker build -f assets/container/Dockerfile -t netools .
#
# Run (mount your data and pass a subcommand):
#   docker run --rm -v "$PWD:/data" netools stats /data/genome.net
#   docker run --rm netools --help
#
# All dependencies are pure Rust (gzip uses the zlib-rs backend), so no system
# libraries are needed at build or run time.

# ---- build stage ----------------------------------------------------------
FROM rust:1.93.0-slim-bookworm AS builder
WORKDIR /src
COPY netools/ ./

RUN cargo build --release --all-features --bin netools --locked && \
    strip target/release/netools

# ---- runtime stage --------------------------------------------------------
FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="netools" \
      org.opencontainers.image.description="work with .net files in Rust" \
      org.opencontainers.image.licenses="GPL-3.0"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    procps \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/netools /usr/local/bin/netools
RUN chmod +x /usr/local/bin/netools
ENV PATH="/usr/local/bin:${PATH}"

RUN netools --version

CMD ["bash"]
