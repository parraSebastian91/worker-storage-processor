# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.88-slim AS builder

WORKDIR /app

# Instalar dependencias del sistema necesarias para compilar
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copiar manifiestos primero para aprovechar la caché de dependencias
COPY Cargo.toml Cargo.lock ./

# Crear un main.rs dummy para compilar dependencias en caché
RUN mkdir -p src && echo "fn main() {}" > src/main.rs \
    && mkdir -p cmd/api && echo "fn main() {}" > cmd/api/main.rs

RUN cargo build --release 2>/dev/null || true
RUN rm -rf src cmd

# Copiar el código fuente real
COPY src ./src
COPY cmd ./cmd

# Compilar el binario en modo release
RUN cargo build --release --bin worker-storage-processor

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Instalar librerías runtime necesarias
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copiar únicamente el binario compilado
COPY --from=builder /app/target/release/worker-storage-processor ./worker-storage-processor

# Usuario no-root por seguridad
RUN useradd --no-create-home --shell /bin/false appuser \
    && chown appuser:appuser ./worker-storage-processor

USER appuser

EXPOSE 3101

CMD ["./worker-storage-processor"]
