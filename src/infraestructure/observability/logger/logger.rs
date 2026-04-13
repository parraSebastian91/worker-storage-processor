//! Logging Configuration

use std::io::IsTerminal;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Inicializa el sistema de logging
pub fn init_logger(log_level: &str, log_format: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));
    let stderr_is_tty = std::io::stderr().is_terminal();
    
    match log_format.to_ascii_lowercase().as_str() {
        "json" => {
            // Formato JSON para producción
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json().with_ansi(false))
                .init();
        }
        "pretty" if stderr_is_tty => {
            // Formato pretty en desarrollo interactivo
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().pretty().with_ansi(true))
                .init();
        }
        "pretty" => {
            // En entornos no interactivos, degradamos pretty a compacto para mayor legibilidad
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().compact().with_ansi(false))
                .init();
        }
        "compact" => {
            // Formato de texto compacto en una sola línea
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().compact().with_ansi(stderr_is_tty))
                .init();
        }
        _ => {
            // Fallback seguro para entornos no interactivos
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().compact().with_ansi(stderr_is_tty))
                .init();
        }
    }
    
    tracing::info!(
        "Logger inicializado - Nivel: {}, Formato: {}, TTY: {}",
        log_level,
        log_format,
        stderr_is_tty
    );
}
