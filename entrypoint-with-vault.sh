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
    # Dos buckets, partidos por quién puede ver el objeto. El eje
    # original/procesado se eliminó: no aporta control de acceso —el bucket es
    # la unidad de permiso en S3— y la etapa del objeto va en la key.
    export STORAGE_BUCKET_PUBLIC="${STORAGE_BUCKET_PUBLIC:-seis-app-public}"
    export STORAGE_BUCKET_PRIVATE="${STORAGE_BUCKET_PRIVATE:-seis-app-private}"
}

load_rabbit_env(){
    echo "  🔑 Cargando secrets de RabbitMQ..."
    local path="secret/data/flowis/rabbit"
    local host=$(vault_get "$path" "RABBITMQ_HOST")
    local port=$(vault_get "$path" "RABBITMQ_PORT")
    local user=$(vault_get "secret/data/flowis/$SERVICE_NAME" "RABBITMQ_USER")
    local pass=$(vault_get "secret/data/flowis/$SERVICE_NAME" "RABBITMQ_PASS")
    export RABBITMQ_URL="amqp://${user}:${pass}@${host}:${port}/%2f"
    # El exchange DE TRABAJO. En Vault, `RABBITMQ_EXCHANGE` es el de
    # NOTIFICACIONES (`storage_notifications_exchange`), que es el canal de
    # vuelta hacia el navegador. El worker consume del de tareas, donde el
    # orquestador publica lo que hay que procesar.
    local exch=$(vault_get "$path" "RABBITMQ_EXCHANGE_TASK")
    export RABBITMQ_EXCHANGE_TASK="${exch:-storage_tasks_exchange}"

    # La cola DE TRABAJO y su routing key.
    #
    # Vault tenía `notify_queue`, que es la cola de NOTIFICACIONES —la que
    # consume el BFF para avisarle al navegador— no la de trabajo. Con eso el
    # worker escuchaba el canal equivocado: el orquestador publicaba en
    # `storage_tasks_exchange` con la clave `media.document.upload`, ese exchange
    # no tenía ningún binding, y RabbitMQ descartaba cada mensaje en silencio.
    #
    # Se toma de Vault si está, y si no se usa el valor correcto. El worker
    # declara y liga la cola al arrancar, así que no hace falta crearla a mano.
    local cola=$(vault_get "$path" "RABBITMQ_QUEUE_DOCUMENT")
    export RABBITMQ_QUEUE="${cola:-media_document_queue}"
    local clave=$(vault_get "$path" "RABBITMQ_KEY_MEDIA_DOCUMENT_UPLOAD")
    export RABBITMQ_ROUTING_KEY="${clave:-media.document.upload}"
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
