# ============================================================
# Dockerfile multi-plataforma - Desarrollo
# Compatible con: Apple M2 (arm64) y AMD Ryzen 9 (amd64)
#
# Uso en M2 Mac:
#   docker build --platform linux/arm64 -t worker-storage-processor .
#
# Uso en Ryzen 9 (amd64):
#   docker build --platform linux/amd64 -t worker-storage-processor .
#
# O simplemente:
#   docker build -t worker-storage-processor .
#   (Docker detecta la plataforma nativa automáticamente)
# ============================================================

# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.88-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    clang \
    mold \
    && rm -rf /var/lib/apt/lists/*

# Perfil de build: "dev-fast" para desarrollo, "release" para prod.
# Por defecto usamos dev-fast (rebuilds rapidos, sin LTO).
ARG BUILD_PROFILE=dev-fast

# RUSTFLAGS:
# - mold: linker hasta 8x más rápido que el linker por defecto (gana mucho en Ryzen y M2)
# - target-cpu=native: usa instrucciones SIMD del CPU actual (NEON en M2, AVX2 en Ryzen 9)
# - strip=symbols: binario más pequeño sin depender de binutils en runtime
ARG RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold -C target-cpu=native -C strip=symbols"
ENV RUSTFLAGS=${RUSTFLAGS}

# Copiar manifiestos primero para aprovechar cache de layers
COPY Cargo.toml Cargo.lock ./

# Cache de dependencias con el perfil correcto
# Se invalida solo si cambia Cargo.toml o Cargo.lock
RUN mkdir -p src cmd/api && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > cmd/api/main.rs && \
    cargo build --profile ${BUILD_PROFILE} --bin worker-storage-processor 2>/dev/null || true && \
    rm -rf src cmd

# Copiar código fuente real
COPY src ./src
COPY cmd ./cmd

# Tocar los archivos fuente para que Cargo detecte el cambio
# (necesario por el truco del dummy build)
RUN touch src/lib.rs cmd/api/main.rs 2>/dev/null || true

# Compilar
RUN cargo build --profile ${BUILD_PROFILE} --bin worker-storage-processor

# Determinar ruta del binario según perfil
# dev-fast → target/dev-fast/  |  release → target/release/
RUN cp target/${BUILD_PROFILE}/worker-storage-processor /app/worker-storage-processor

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/worker-storage-processor ./

RUN useradd --no-create-home --shell /bin/false appuser && \
    chown appuser:appuser ./worker-storage-processor

USER appuser

CMD ["./worker-storage-processor"]