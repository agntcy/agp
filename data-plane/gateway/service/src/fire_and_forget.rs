// Copyright AGNTCY Contributors (https://github.com/agntcy)
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;
use tonic::Status;

use crate::session::SessionType;
use crate::session::{Common, Error, Id, Info, MessageDirection, Session, SessionDirection, State};
use crate::traits;
use agp_datapath::messages::utils;
use agp_datapath::pubsub::proto::pubsub::v1::Message;
use agp_datapath::pubsub::proto::pubsub::v1::SessionHeaderType;

pub(crate) struct FireAndForget {
    common: Common,
}

impl FireAndForget {
    pub(crate) fn new(
        id: Id,
        session_direction: SessionDirection,
        tx_gw: tokio::sync::mpsc::Sender<Result<Message, Status>>,
    ) -> FireAndForget {
        FireAndForget {
            common: Common::new(id, session_direction, tx_gw, None),
        }
    }
}

#[async_trait]
impl Session for FireAndForget {
    fn id(&self) -> Id {
        self.common.id()
    }

    fn state(&self) -> &State {
        self.common.state()
    }

    fn session_type(&self) -> SessionType {
        SessionType::FireAndForget
    }

    async fn open(
        &mut self,
        _listener: Arc<dyn traits::Listener>,
    ) -> Result<(Info, tokio::sync::mpsc::Receiver<(Message, Info)>), Error> {
        // Create channel to communicate with the app
        let (tx_app, rx_app) = tokio::sync::mpsc::channel(128);

        // set tx_app
        self.common.set_tx_app(tx_app);

        // Get session id
        let id = self.common.id();

        // Get session state
        let state = self.common.state().clone();

        // Call the open_channel method on the listener
        return Ok((Info::new(id, SessionType::FireAndForget, state), rx_app));
    }

    async fn on_message(
        &self,
        mut message: Message,
        direction: MessageDirection,
    ) -> Result<Option<Message>, Error> {
        // set the session type
        let header = utils::get_session_header_as_mut(&mut message);
        if header.is_none() {
            return Err(Error::AppTransmission("missing header".to_string()));
        }

        header.unwrap().header_type = utils::service_type_to_int(SessionHeaderType::Fnf);

        // clone tx
        match direction {
            MessageDirection::North => {
                // create info
                let info = Info::new(
                    self.common.id(),
                    SessionType::FireAndForget,
                    self.common.state().clone(),
                );

                let tx = self.common.tx_app().expect("tx_app is None");
                tx.send((message, info))
                    .await
                    .map(|_| None)
                    .map_err(|e| Error::AppTransmission(e.to_string()))
            }
            MessageDirection::South => {
                // add a nonce to the message
                let header = utils::get_session_header_as_mut(&mut message).unwrap();
                header.message_id = rand::rng().random();

                let tx = self.common.tx_gw();
                tx.send(Ok(message))
                    .await
                    .map(|_| None)
                    .map_err(|e| Error::GatewayTransmission(e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agp_datapath::messages::encoder;

    use crate::test_utils::TestListener;

    #[tokio::test]
    async fn test_fire_and_forget_create() {
        // create a fake gateway sender
        let (tx_gw, _rx_gw) = tokio::sync::mpsc::channel(1);

        let session = FireAndForget::new(0, SessionDirection::Bidirectional, tx_gw);

        assert_eq!(session.id(), 0);
        assert_eq!(session.state(), &State::Active);
        assert_eq!(session.session_type(), SessionType::FireAndForget);
    }

    #[tokio::test]
    async fn test_fire_and_forget_on_message() {
        // create fake gateway sender
        let (tx_gw, _rx_gw) = tokio::sync::mpsc::channel(1);

        // create listener
        let listener = Arc::new(TestListener::new());

        // create session
        let mut session = FireAndForget::new(0, SessionDirection::Bidirectional, tx_gw);

        // open the session and get the rx channel
        let (_info, mut stream) = session
            .open(listener.clone())
            .await
            .expect("error opening session");

        // create a message
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

        // emulate the send of the message to the app
        let res = session
            .on_message(message.clone(), MessageDirection::North)
            .await;
        assert!(res.is_ok());

        // wait for the message to be received by the app
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // make sure the app received the message
        let (msg, info) = stream.recv().await.expect("no message received");
        assert_eq!(msg, message);
        assert_eq!(info.id, 0);
        assert_eq!(info.session_type, SessionType::FireAndForget);
        assert_eq!(info.state, State::Active);
    }
}
