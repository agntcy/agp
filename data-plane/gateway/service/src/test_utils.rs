// Copyright AGNTCY Contributors (https://github.com/agntcy)
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::session::Info;
use crate::traits::{AppInputStream, Listener};
use crate::ServiceError;
use agp_datapath::pubsub::proto::pubsub::v1::Message;

// create a fake listener for testing
pub(crate) struct TestListener {
    sessions: RwLock<HashMap<u32, ()>>,
    rx: RwLock<tokio::sync::mpsc::Receiver<(Message, Info)>>,
    tx: tokio::sync::mpsc::Sender<(Message, Info)>,
}

impl TestListener {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel::<(Message, Info)>(128);

        Self {
            sessions: RwLock::new(HashMap::new()),
            rx: RwLock::new(rx),
            tx,
        }
    }

    pub async fn get_message(&self) -> (Message, Info) {
        self.rx.write().await.recv().await.unwrap()
    }

    pub fn start_receive_loop(&self, stream: AppInputStream) {
        // spawn task to read from the app and increment the message counter
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut stream = stream;
            while let Some((msg, info)) = stream.recv().await {
                tx.send((msg, info)).await.unwrap();
            }
        });
    }
}

#[async_trait]
impl Listener for TestListener {
    async fn open_channel(
        &self,
        session: Info,
        stream: AppInputStream,
    ) -> Result<(), ServiceError> {
        // get writer lock
        let mut session_locked = self.sessions.write().await;

        // insert the session id and the tx channel
        session_locked.insert(session.id, ());

        // spawn task to read from the app and increment the message counter
        Ok(self.start_receive_loop(stream))
    }

    async fn request_reply(
        &self,
        _session: Info,
        _message: Message,
    ) -> Result<Message, ServiceError> {
        Err(ServiceError::Unknown)
    }
}
