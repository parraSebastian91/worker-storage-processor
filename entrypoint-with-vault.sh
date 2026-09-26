#!/bin/bash
# entrypoint-with-vault.sh
# Carga secrets de Vault y ejecuta app (mismo lineamiento que ms-identity, ms-core, bff y ms-storage)

set -e

echo "🔐 Cargando secrets desde Vault..."

VAULT_ADDR="${VAULT_ADDR:-http://vault_server:8200}"
VAULT_TOKEN="${VAULT_TOKEN:-}"

if [ -z "$VAULT_TOKEN" ]; then
    echo "❌ VAULT_TOKEN no configurado"
    exit 1
fi

vault_get() {
    local path=$1
    local field=$2
    curl -sf -H "X-Vault-Token: $VAULT_TOKEN" \
        "$VAULT_ADDR/v1/$path" | \
        jq -r ".data.data.$field // empty"
}

load_database(){
    echo "  🔑 Cargando secrets de base de datos..."
    local path="secret/data/flowis/postgres"
    export DB_HOST=$(vault_get "$path" "DATABASE_HOST")
    export DB_PORT=$(vault_get "$path" "DATABASE_PORT")
    export DB_USER=$(vault_get "$path" "DATABASE_USER")
    export DB_PASSWORD=$(vault_get "$path" "DATABASE_PASSWORD")
    export DB_NAME=$(vault_get "$path" "DATABASE_NAME")
}

load_redis(){
    echo "  🔑 Cargando secrets de Redis..."
    local path="secret/data/flowis/redis"
    export CACHE_HOST=$(vault_get "$path" "REDIS_HOST")
    export CACHE_PORT=$(vault_get "$path" "REDIS_PORT")
    export CACHE_DB=$(vault_get "$path" "REDIS_DB")
    export CACHE_PASSWORD=$(vault_get "$path" "REDIS_PASS")
}

load_storage_minio(){
    echo "  🔑 Cargando secrets de MinIO..."
    local path="secret/data/flowis/storage_minio"
    export MINIO_ACCESS_KEY=$(vault_get "$path" "MINIO_ROOT_USER")
    export MINIO_SECRET_KEY=$(vault_get "$path" "MINIO_ROOT_PASSWORD")
    export MINIO_URL_BASE=$(vault_get "$path" "MINIO_ENDPOINT")
    export STORAGE_BUCKET_PUBLIC_ORIGINAL="${STORAGE_BUCKET_PUBLIC_ORIGINAL:-seis-app-public-original}"
    export STORAGE_BUCKET_PUBLIC_PROCESSED="${STORAGE_BUCKET_PUBLIC_PROCESSED:-seis-app-public-processed}"
    export STORAGE_BUCKET_PRIVATE_ORIGINAL="${STORAGE_BUCKET_PRIVATE_ORIGINAL:-seis-app-private-original}"
    export STORAGE_BUCKET_PRIVATE_PROCESSED="${STORAGE_BUCKET_PRIVATE_PROCESSED:-seis-app-private-processed}"
}

load_rabbit_env(){
    echo "  🔑 Cargando secrets de RabbitMQ..."
    local path="secret/data/flowis/rabbit"
    local host=$(vault_get "$path" "RABBITMQ_HOST")
    local port=$(vault_get "$path" "RABBITMQ_PORT")
    local user=$(vault_get "secret/data/flowis/$SERVICE_NAME" "RABBITMQ_USER")
    local pass=$(vault_get "secret/data/flowis/$SERVICE_NAME" "RABBITMQ_PASS")
    export RABBITMQ_URL="amqp://${user}:${pass}@${host}:${port}/%2f"
    export RABBITMQ_QUEUE=$(vault_get "$path" "RABBITMQ_QUEUE")
    export RABBITMQ_EXCHANGE_TASK=$(vault_get "$path" "RABBITMQ_EXCHANGE")
}

load_service_env(){
    SERVICE_NAME="${SERVICE_NAME:-worker-storage-processor}"
    echo "  📦 Cargando secrets para $SERVICE_NAME..."
    load_database
    load_redis
    load_storage_minio
    load_rabbit_env
    export EXTERNAL_ORCHESTRATOR_URL=$(vault_get "secret/data/flowis/external_services" "STORAGE_SERVICE_BASE_URL")
    export PORT=$(vault_get "secret/data/flowis/$SERVICE_NAME" "PORT")
    export PORT="${PORT:-3101}"
    export LOG_LEVEL=$(vault_get "secret/data/flowis/$SERVICE_NAME" "MIN_LOG_LEVEL")
    export LOG_LEVEL="${LOG_LEVEL:-info}"
}

load_service_env
echo "🚀 Iniciando aplicación..."
echo ""

exec "$@"
