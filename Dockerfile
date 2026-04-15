# ============================================================
# Stage 1: Build (Optimizado para CPU)
# ============================================================
FROM rust:1.88-slim AS builder

WORKDIR /app

# Agregamos clang y llvm por si la crate 'webp' necesita generar bindings
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Copiar manifiestos
COPY Cargo.toml Cargo.lock ./

# Cache de dependencias (truco del dummy main mejorado)
RUN mkdir -p src cmd/api && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > cmd/api/main.rs && \
    cargo build --release && \
    rm -rf src cmd

# Copiar código fuente real
COPY src ./src
COPY cmd ./cmd

# --- OPTIMIZACIONES DE CPU ---
# Usamos RUSTFLAGS para habilitar instrucciones vectoriales (SIMD).
# 'target-cpu=native' es lo mejor si compilas en la misma máquina donde ejecutas.
# Si vas a desplegar en la nube (AWS/GCP), 'x86-64-v3' activa AVX2.
# 'strip=symbols' elimina simbolos en build y evita depender de binutils en runtime.
ENV RUSTFLAGS="-C target-cpu=x86-64-v3 -C strip=symbols"

# Compilar con LTO (Link Time Optimization) para reducir tamaño y ganar velocidad
RUN cargo build --release --bin worker-storage-processor

# ============================================================
# Stage 2: Runtime (Liviano y Seguro)
# ============================================================
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Solo lo mínimo para correr
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/worker-storage-processor ./

RUN useradd --no-create-home --shell /bin/false appuser && \
    chown appuser:appuser ./worker-storage-processor

USER appuser

CMD ["./worker-storage-processor"]