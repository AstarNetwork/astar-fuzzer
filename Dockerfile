FROM rust:1.86.0-slim as base

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    llvm \
    git \
    pkg-config \
    libssl-dev \
    binutils-dev \
    libbfd-dev \
    libunwind-dev \
    gnuplot \
    gdb \
    && rm -rf /var/lib/apt/lists/*

# Add WASM target
RUN rustup target add wasm32-unknown-unknown && \
    rustup component add rust-src

# Install ziggy and fuzzing tools (this is slow, so do it once)
RUN cargo install ziggy cargo-afl@0.15.19 honggfuzz grcov casr
# Stage 2: Development (rebuild frequently)
FROM base as development

WORKDIR /app

# Copy dependency files first (for better caching)
COPY Cargo.toml Cargo.lock ./

# Pre-build dependencies (this will be cached if deps don't change)
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    SKIP_WASM_BUILD=1 cargo build && \
    rm -rf src

# Copy source code
COPY src/ ./src/

# Build the fuzzer
RUN SKIP_WASM_BUILD=1 cargo ziggy build

CMD ["bash"]
