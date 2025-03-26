// Copyright AGNTCY Contributors (https://github.com/agntcy)
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;

use crate::{session, ServiceError};
use agp_datapath::pubsub::proto::pubsub::v1::Message;

pub type SessionMessageSingle = (Message, session::Info);
pub type AppInputStream = tokio::sync::mpsc::Receiver<SessionMessageSingle>;

#[async_trait]
pub trait Listener: Send + Sync {
    async fn open_channel(
        &self,
        session: session::Info,
        stream: AppInputStream,
    ) -> Result<(), ServiceError>;

    async fn request_reply(
        &self,
        session: session::Info,
        message: Message,
    ) -> Result<Message, ServiceError>;
}
