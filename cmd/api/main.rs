use axum::{routing::get, Router};
use worker_storage_processor::aplication::service::event_manager_service::EventManagerService;
use worker_storage_processor::aplication::service::image_manager_service::ImageManagerService;
use worker_storage_processor::domain::ports::outbound::object_db_repository::IObjectDBRepository;
use worker_storage_processor::infraestructure::adapter::inbound::http::controller::healthcheck_controller::{
    liveness, readiness, HealthcheckState,
};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tracing::{error, info};

use worker_storage_processor::aplication::use_case::event_manager_usecase::EventManagerUseCase;
use worker_storage_processor::domain::ports::inbound::event_manager_usecase::IEventManagerUseCase;
use worker_storage_processor::domain::ports::inbound::queue_handler::IQueueConsumer;
use worker_storage_processor::domain::ports::outbound::object_storage_repository::IObjectStorageRepository;
use worker_storage_processor::infraestructure::adapter::inbound::queue::queue_handler::QueueHandler;
use worker_storage_processor::infraestructure::adapter::inbound::queue::rabbitmq::queue_client::RabbitMQConsumerImpl;
use worker_storage_processor::infraestructure::adapter::outbound::db::cache::cache_client::RedisCacheImpl;
use worker_storage_processor::infraestructure::adapter::outbound::db::postgres::postgres_client::PostgresClient;
use worker_storage_processor::infraestructure::adapter::outbound::storage::minio::minio_client::MinioClientAdapter;
use worker_storage_processor::infraestructure::{
    config::app_config::AppConfig, observability::logger::logger::init_logger,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    info!("worker-storage-processor iniciado correctamente");
    info!("Cargando configuración desde variables de entorno...");

    // ============== Cargando configuración ==============
    let config = match load_config_from_env() {
        Ok(cfg) => {
            eprintln!("✅ Configuración cargada exitosamente");
            cfg
        }
        Err(e) => {
            eprintln!("❌ Error cargando configuración: {}", e);
            return Err(e);
        }
    };

    // ============== Inicializando logger ==============

    setup_observability(&config);
    info!("🚀 Iniciando Worker Document");

    // ============== Inicializando Clientes outbound ==============

    let _storage_clients = init_storage_client(&config).await?;
    let _database = init_database_client(&config).await?;
    let _cache_client = init_cache_client(&config).await?;
    let _queue_client = init_queue_client(&config).await?;

    // ========== Iniciador Serivicios de aplicacion ==========

    let _image_manager_service = Arc::new(ImageManagerService::new());

    // =========== Inicializando Handler de la cola ===========

    let _database: Arc<dyn IObjectDBRepository> = _database;

    let event_manager_service = Arc::new(EventManagerService::new(
        _storage_clients.clone(),
        Arc::clone(&_database),
        Arc::clone(&_image_manager_service),
    ));

    let event_manager = Arc::new(<EventManagerUseCase as IEventManagerUseCase>::new(
        _storage_clients,
        _database,
        event_manager_service,
    ));
    let handler: QueueHandler = QueueHandler::new(_queue_client, event_manager);

    // =========== iniciando usecase para el handler de la cola ===========

    run_with_graceful_shutdown(handler, Arc::clone(&config)).await?;

    Ok(())
}

fn load_config_from_env() -> anyhow::Result<Arc<AppConfig>> {
    AppConfig::from_env().map_err(|e| anyhow::anyhow!("Error cargando configuración: {}", e))
}

fn setup_observability(config: &AppConfig) {
    let observability_config = &config.observability;

    init_logger(
        &observability_config.log_level,
        &observability_config.log_format,
    );
    info!("📋 Configuración cargada");
}

async fn init_storage_client(
    config: &AppConfig,
) -> anyhow::Result<HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>> {
    let mut clients: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>> =
        HashMap::new();

    let minio_config = &config.minio_config;
    let minio_client = MinioClientAdapter::new(
        minio_config.url_base.clone(),
        minio_config.bucket.clone(),
        minio_config.access_key.clone(),
        minio_config.secret_key.clone(),
        minio_config.is_principal,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Error inicializando cliente Minio: {}", e))?;
    info!("✅ Cliente Minio inicializado correctamente");
    let minio_client: Arc<dyn IObjectStorageRepository + Send + Sync> = Arc::new(minio_client);
    clients.insert("minio".to_string(), minio_client);

    Ok(clients)
}

async fn init_database_client(config: &AppConfig) -> anyhow::Result<Arc<PostgresClient>> {
    let db_config = &config.db_config;
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db_config.user, db_config.password, db_config.host, db_config.port, db_config.database
    );

    info!("Conectando a Postgres con URL: {}", database_url);
    let postgres_client = PostgresClient::new(database_url, db_config.max_connections)
        .await
        .map_err(anyhow::Error::msg)?;
    info!("✅ Cliente Postgres inicializado correctamente");

    Ok(Arc::new(postgres_client))
}

async fn init_cache_client(config: &AppConfig) -> anyhow::Result<Arc<RedisCacheImpl>> {
    let cache_config = &config.cache_config;
    let cache_client = RedisCacheImpl::new(
        &cache_config.host,
        cache_config.port,
        &cache_config.password,
        cache_config.db,
        cache_config.ttl,
    )
    .await?;
    info!("✅ Cliente Redis Cache inicializado correctamente");

    Ok(Arc::new(cache_client))
}

async fn init_queue_client(config: &AppConfig) -> anyhow::Result<Arc<dyn IQueueConsumer>> {
    info!("Inicializando cliente de cola...");
    let queue_config = &config.queue_config;
    let queue_client = RabbitMQConsumerImpl::new(
        &queue_config.url,
        &queue_config.exchange,
        &queue_config.queue,
        queue_config.prefetch_count,
        queue_config.max_retries,
    )
    .await?;
    // Aquí se implementaría la inicialización del cliente de la cola (RabbitMQ, Kafka, etc.)
    // Por ejemplo:
    // let queue_client = RabbitMQClient::new(&config.rabbitmq_config).await?;
    // info!("✅ Cliente RabbitMQ inicializado correctamente");
    // Ok(Arc::new(queue_client))

    Ok(Arc::new(queue_client)) // Placeholder
}

// ============================================================================
// EJECUCIÓN Y GRACEFUL SHUTDOWN
// ============================================================================

/// Ejecuta el worker con manejo de graceful shutdown
async fn run_with_graceful_shutdown(
    handler: QueueHandler,
    config: Arc<AppConfig>,
) -> anyhow::Result<()> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Capturar Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("🛑 Señal de cierre recibida");
        let _ = shutdown_tx.send(()).await;
    });

    // Iniciar Worker
    let mut worker_handle = tokio::spawn(async move {
        if let Err(e) = handler.run().await {
            error!("❌ Error en worker: {}", e);
        }
    });

    // Iniciar Servicio Healthcheck
    let service_config = Arc::clone(&config);
    let mut service_handle = tokio::spawn(async move {
        let state = HealthcheckState {
            config: Arc::clone(&service_config),
        };

        let app = Router::new()
            .route("/liveness", get(liveness))
            .route("/readiness", get(readiness))
            .with_state(state);

        // Parseo seguro del puerto
        let port: u16 = service_config
            .configuracion_gral
            .port
            .parse()
            .map_err(|e| anyhow::anyhow!("Puerto inválido: {}", e))?;
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);

        let listener = tokio::net::TcpListener::bind(socket)
            .await
            .map_err(|e| anyhow::anyhow!("Error al bindear puerto {}: {}", port, e))?;

        info!(
            "✅ Healthcheck escuchando en 0.0.0.0:{} (/liveness, /readiness)",
            port
        );

        axum::serve(listener, app)
            .await
            .map_err(|e| anyhow::anyhow!("Error en servidor healthcheck: {}", e))
    });

    // Esperar señal de cierre o terminación inesperada de tareas
    tokio::select! {
        _ = shutdown_rx.recv() => {
            println!("🔄 Deteniendo worker gracefully...");
        }
        result = &mut service_handle => {
            match result {
                Ok(Ok(())) => {
                    return Err(anyhow::anyhow!("Servidor healthcheck finalizó inesperadamente"));
                }
                Ok(Err(e)) => {
                    return Err(e);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Task healthcheck falló: {}", e));
                }
            }
        }
        result = &mut worker_handle => {
            match result {
                Ok(()) => {
                    return Err(anyhow::anyhow!("Worker finalizó inesperadamente"));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Task worker falló: {}", e));
                }
            }
        }
    }

    worker_handle.abort();
    service_handle.abort();

    Ok(())
}
