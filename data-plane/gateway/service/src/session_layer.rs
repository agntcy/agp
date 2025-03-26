// Copyright AGNTCY Contributors (https://github.com/agntcy)
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use rand::Rng;
use tokio::sync::{mpsc::Sender, RwLock};
use tonic::Status;

use crate::fire_and_forget;
use crate::session::{Error, Id, Info, MessageDirection, Session, SessionDirection, SessionType};
use crate::traits;
use agp_datapath::messages::utils;
use agp_datapath::pubsub::proto::pubsub::v1::Message;
use agp_datapath::pubsub::proto::pubsub::v1::SessionHeaderType;

/// SessionLayer
pub(crate) struct SessionLayer {
    /// Session pool
    pool: RwLock<HashMap<Id, Box<dyn Session + Send + Sync>>>,

    /// ID of the local connection
    conn_id: u64,

    /// Tx channel to gateway
    tx_gw: Sender<Result<Message, Status>>,

    /// Listener
    listener: Arc<dyn traits::Listener>,
}

impl std::fmt::Debug for SessionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SessionPool")
    }
}

impl SessionLayer {
    /// Create a new session pool
    pub(crate) fn new(
        conn_id: u64,
        tx_gw: Sender<Result<Message, Status>>,
        listener: Arc<dyn traits::Listener>,
    ) -> SessionLayer {
        SessionLayer {
            pool: RwLock::new(HashMap::new()),
            conn_id,
            tx_gw,
            listener,
        }
    }

    pub(crate) fn tx_gw(&self) -> Sender<Result<Message, Status>> {
        self.tx_gw.clone()
    }

    pub(crate) fn conn_id(&self) -> u64 {
        self.conn_id
    }

    /// Insert a new session into the pool
    pub(crate) async fn insert_session(
        &self,
        id: Id,
        session: Box<dyn Session + Send + Sync>,
    ) -> Result<(), Error> {
        // get the write lock
        let mut pool = self.pool.write().await;

        // check if the session already exists
        if pool.contains_key(&id) {
            return Err(Error::SessionIdAlreadyUsed(id.to_string()));
        }

        pool.insert(id, session);

        Ok(())
    }

    pub(crate) async fn create_session(
        &self,
        session_type: SessionType,
        id: Option<Id>,
    ) -> Result<(Info, tokio::sync::mpsc::Receiver<(Message, Info)>), Error> {
        // TODO(msardara): the session identifier should be a combination of the
        // session ID and the agent ID, to prevent collisions.

        // generate a new session ID
        let id = match id {
            Some(id) => id,
            None => rand::rng().random(),
        };

        // create a new session
        let mut session = match session_type {
            SessionType::FireAndForget => Box::new(fire_and_forget::FireAndForget::new(
                id,
                SessionDirection::Bidirectional,
                self.tx_gw.clone(),
            )),
            _ => return Err(Error::SessionUnknown(session_type.to_string())),
        };

        // call open on the session, to allow the setup of channels with the app
        let res = session.open(self.listener.clone()).await;

        // in case of error, return it
        if let Err(e) = res {
            return Err(e);
        }

        // insert the session into the pool
        self.insert_session(id, session).await?;

        // return the session info
        res
    }

    /// Remove a session from the pool
    pub(crate) async fn remove_session(&self, id: Id) -> bool {
        // get the write lock
        let mut pool = self.pool.write().await;
        pool.remove(&id).is_some()
    }

    /// Handle messages from the message processor
    pub(crate) async fn handle_message(
        &self,
        message: Message,
        direction: MessageDirection,
        session_id: Option<Id>,
    ) -> Result<Option<Message>, Error> {
        match direction {
            MessageDirection::North => self.handle_message_from_gateway(message, direction).await,
            MessageDirection::South => {
                // make sure the session ID is provided
                let session_id = match session_id {
                    Some(id) => id,
                    None => return Err(Error::MissingSessionId("none".to_string())),
                };

                self.handle_message_from_app(message, direction, session_id)
                    .await
            }
        }
    }

    /// Handle a message from the message processor, and pass it to the
    /// corresponding session
    async fn handle_message_from_app(
        &self,
        mut message: Message,
        direction: MessageDirection,
        session_id: Id,
    ) -> Result<Option<Message>, Error> {
        // check if pool contains the session
        if let Some(session) = self.pool.read().await.get(&session_id) {
            // Set session id and session type to message
            let header = utils::get_session_header_as_mut(&mut message);
            if header.is_none() {
                return Err(Error::MissingSessionHeader);
            }

            let header = header.unwrap();
            header.session_id = session_id;

            // pass the message to the session
            return session.on_message(message, direction).await;
        }

        // if the session is not found, return an error
        Err(Error::SessionNotFound(session_id.to_string()))
    }

    /// Handle a message from the message processor, and pass it to the
    /// corresponding session
    async fn handle_message_from_gateway(
        &self,
        message: Message,
        direction: MessageDirection,
    ) -> Result<Option<Message>, Error> {
        let (id, session_type) = {
            // get the session type and the session id from the message
            let header = utils::get_session_header(&message);

            // if header is None, return an error
            if header.is_none() {
                return Err(Error::MissingAgpHeader("missing AGP header".to_string()));
            }

            let header = header.unwrap();

            // get the session type from the header
            let session_type = utils::int_to_service_type(header.header_type);

            // if the session type is not specified, return an error
            if session_type.is_none() {
                return Err(Error::SessionUnknown(header.header_type.to_string()));
            }

            // get the session ID
            let id = header.session_id;

            (id, session_type.unwrap())
        };

        // check if pool contains the session
        if let Some(session) = self.pool.read().await.get(&id) {
            // pass the message to the session
            let ret = session.on_message(message, direction).await;
            return ret;
        }

        let (info, stream) = match session_type {
            SessionHeaderType::Fnf => {
                self.create_session(SessionType::FireAndForget, Some(id))
                    .await?
            }
            _ => {
                return Err(Error::SessionUnknown(
                    session_type.as_str_name().to_string(),
                ))
            }
        };

        debug_assert!(info.id == id);

        // As we are handling the message from the gateway, we need to call the
        // open_channel method on the listener
        self.listener
            .open_channel(info, stream)
            .await
            .map_err(|e| Error::AppReception(e.to_string()))?;

        // retry the match
        if let Some(session) = self.pool.write().await.get_mut(&id) {
            // pass the message
            return session.on_message(message, direction).await;
        }

        // this should never happen
        panic!("session not found: {}", "test");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::fire_and_forget::FireAndForget;
    use crate::session;
    use crate::test_utils::TestListener;
    use agp_datapath::messages::encoder;

    fn create_session_layer() -> SessionLayer {
        // create listener
        let listener = Arc::new(TestListener::new());

        // create fake channel to gateway
        let (tx_gw, _) = tokio::sync::mpsc::channel(1);

        SessionLayer::new(0, tx_gw, listener)
    }

    #[tokio::test]
    async fn test_create_session_layer() {
        let session_layer = create_session_layer();

        assert!(session_layer.pool.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_insert_session() {
        let session_layer = create_session_layer();

        let session = Box::new(FireAndForget::new(
            1,
            SessionDirection::Bidirectional,
            session_layer.tx_gw(),
        ));

        let res = session_layer.insert_session(1, session).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_remove_session() {
        let session_layer = create_session_layer();

        let session = Box::new(FireAndForget::new(
            1,
            SessionDirection::Bidirectional,
            session_layer.tx_gw(),
        ));

        session_layer.insert_session(1, session).await.unwrap();
        let res = session_layer.remove_session(1).await;

        assert!(res);
    }

    #[tokio::test]
    async fn test_create_session() {
        let session_layer = create_session_layer();

        let res = session_layer
            .create_session(SessionType::FireAndForget, None)
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_handle_message() {
        // create listener
        let listener = Arc::new(TestListener::new());

        // create fake channel to gateway
        let (tx_gw, _) = tokio::sync::mpsc::channel(1);

        let session_layer = SessionLayer::new(0, tx_gw, listener.clone());

        let mut session = Box::new(FireAndForget::new(
            1,
            SessionDirection::Bidirectional,
            session_layer.tx_gw(),
        ));

        let (_info, mut stream) = session.open(listener.clone()).await.unwrap();

        session_layer.insert_session(1, session).await.unwrap();

        let mut message = utils::create_publication(
            &encoder::encode_agent("cisco", "default", "local_agent", 0),
            &encoder::encode_agent_type("cisco", "default", "remote_agent"),
            Some(0),
            None,
            None,
            1,
            "msg",
            vec![0x1, 0x2, 0x3, 0x4],
        );

        // set the session id in the message
        let header = utils::get_session_header_as_mut(&mut message).unwrap();
        header.session_id = 1;

        let res = session_layer
            .handle_message(message.clone(), MessageDirection::North, Some(1))
            .await;

        assert!(res.is_ok());

        // message should have been delivered to the app
        let (msg, info) = stream.recv().await.expect("no message received");
        assert_eq!(msg, message);
        assert_eq!(info.id, 1);
        assert_eq!(info.session_type, SessionType::FireAndForget);
        assert_eq!(info.state, session::State::Active);
    }
}
