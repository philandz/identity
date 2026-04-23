use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("queue closed")]
    Closed,
}

pub type QueueSender<T> = mpsc::Sender<T>;
pub type QueueReceiver<T> = mpsc::Receiver<T>;

pub fn bounded<T>(capacity: usize) -> (QueueSender<T>, QueueReceiver<T>) {
    mpsc::channel(capacity)
}

pub async fn enqueue<T>(tx: &QueueSender<T>, item: T) -> Result<(), QueueError> {
    tx.send(item).await.map_err(|_| QueueError::Closed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_receive() {
        let (tx, mut rx) = bounded(4);
        enqueue(&tx, 42).await.expect("send");
        assert_eq!(rx.recv().await, Some(42));
    }
}
