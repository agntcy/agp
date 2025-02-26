// SPDX-FileCopyrightText: Copyright (c) 2025 Cisco and/or its affiliates.
// SPDX-License-Identifier: Apache-2.0

use std::str;

use agp_datapath::messages::encoder::encode_agent_from_string;
use agp_gw::config;
use tracing::info;


#[tokio::main]
async fn main() {
    let config_file = "../config/base/client-config.yaml";
    let id = 1;

    // setup agent config
    let mut config = config::load_config(config_file).expect("failed to load configuration");
    let _guard = config.tracing.setup_tracing_subscriber();

    
    // start local agent
    // get service
    let svc_id = agp_config::component::id::ID::new_with_str("gateway/0").unwrap();
    let svc = config.services.get_mut(&svc_id).unwrap();

    // create local agent
    let agent_name = encode_agent_from_string("cisco", "default", "mpc_client", id);
    let mut rx = svc.create_agent(agent_name.clone());

    // connect to the remote gateway
    let conn_id = svc.connect(None).await.unwrap();
    info!("remote connection id = {}", conn_id);

    // wait for the connection to be established
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // send subscription
    let _ = svc.subscribe(&agent_name.agent_class, Some(id), conn_id).await;

    // send request
    let dest_name = encode_agent_from_string("cisco", "default", "mpc_server", 0);
    svc.send_msg(&dest_name.agent_class, None, 1, "tools".as_bytes().to_vec(), conn_id).await.unwrap();

    info!("waiting for incoming messages");
    // wait for messages
    let recv_msg = rx.recv().await.unwrap().unwrap();
    match &recv_msg.message_type.unwrap() {
        agp_datapath::pubsub::ProtoPublishType(msg) => {
            let payload = agp_datapath::messages::utils::get_payload(msg);
            let payload_str = str::from_utf8(payload).unwrap();

            info!("receive msg: {}", payload_str);
        }
        t => {
            panic!("received unexpected message: {:?}", t);
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
}
