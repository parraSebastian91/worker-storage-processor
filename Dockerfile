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

ARG ENABLE_OCR=true

# Nota: en Debian slim no hay `libpdfium-dev` oficial en muchos repos.
# Si compilas con OCR, debes proveer libpdfium por otro medio (archivo .so/.dll en runtime).

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        clang \
        mold; \
    if [ "$ENABLE_OCR" = "true" ]; then \
        apt-get install -y --no-install-recommends \
            tesseract-ocr \
            tesseract-ocr-spa \
            libtesseract-dev \
            libleptonica-dev; \
    fi; \
    rm -rf /var/lib/apt/lists/*

# Perfil de build: "dev-fast" para desarrollo, "release" para prod.
# Por defecto usamos dev-fast (rebuilds rapidos, sin LTO).
ARG BUILD_PROFILE=dev-fast
ARG CARGO_FEATURES=""

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
    cargo build --profile ${BUILD_PROFILE} --bin worker-storage-processor ${CARGO_FEATURES} 2>/dev/null || true && \
    rm -rf src cmd

# Copiar código fuente real
COPY src ./src
COPY cmd ./cmd

# Tocar los archivos fuente para que Cargo detecte el cambio
# (necesario por el truco del dummy build)
RUN touch src/lib.rs cmd/api/main.rs 2>/dev/null || true

# Compilar
RUN cargo build --profile ${BUILD_PROFILE} --bin worker-storage-processor ${CARGO_FEATURES}

# Determinar ruta del binario según perfil
# dev-fast → target/dev-fast/  |  release → target/release/
RUN cp target/${BUILD_PROFILE}/worker-storage-processor /app/worker-storage-processor

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

WORKDIR /app

ARG ENABLE_OCR=false
ARG TARGETARCH
ARG PDFIUM_VERSION=7543

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
        tar; \
    if [ "$ENABLE_OCR" = "true" ]; then \
        apt-get install -y --no-install-recommends \
            tesseract-ocr \
            tesseract-ocr-spa \
            libtesseract5 \
            libleptonica-dev; \
        case "${TARGETARCH}" in \
            amd64) PDFIUM_ARCH="linux-x64" ;; \
            arm64) PDFIUM_ARCH="linux-arm64" ;; \
            *) echo "Unsupported TARGETARCH: ${TARGETARCH}"; exit 1 ;; \
        esac; \
        PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_VERSION}/pdfium-${PDFIUM_ARCH}.tgz"; \
        curl -fL "${PDFIUM_URL}" -o /tmp/pdfium.tgz; \
        mkdir -p /tmp/pdfium; \
        tar -xzf /tmp/pdfium.tgz -C /tmp/pdfium; \
        cp /tmp/pdfium/lib/libpdfium.so /usr/lib/libpdfium.so; \
        chmod 755 /usr/lib/libpdfium.so; \
        rm -rf /tmp/pdfium /tmp/pdfium.tgz; \
    fi; \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/worker-storage-processor ./

RUN useradd --no-create-home --shell /bin/false appuser && \
    chown appuser:appuser ./worker-storage-processor

USER appuser

CMD ["./worker-storage-processor"]