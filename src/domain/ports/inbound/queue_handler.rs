use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;

use crate::domain::{
    errors::{consumer_error::ConsumerError, handler_error::HandlerError},
    models::message_event_model::PublishPayload,
};

pub type QueueHandler = Arc<
    dyn Fn(PublishPayload) -> Pin<Box<dyn Future<Output = Result<(), HandlerError>> + Send>>
        + Send
        + Sync,
>;

#[async_trait]
pub trait IQueueConsumer: Send + Sync {
    async fn consume(&self, handler: QueueHandler) -> Result<(), ConsumerError>;
    async fn ack(&self, delivery_tag: u64) -> Result<(), ConsumerError>;
    async fn nack(&self, delivery_tag: u64, requeue: bool) -> Result<(), ConsumerError>;
    async fn close(&self) -> Result<(), ConsumerError>;
}
