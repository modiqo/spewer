use super::{Command, Database, closed};
use crate::error::Result;
use crate::protocol::TaskRequest;
use tokio::sync::oneshot;

impl Database {
    /// Loads the immutable accepted request for recovery.
    pub async fn request(&self, task_id: String) -> Result<TaskRequest> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Request {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }
}
