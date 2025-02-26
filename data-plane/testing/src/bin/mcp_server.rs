// SPDX-FileCopyrightText: Copyright (c) 2025 Cisco and/or its affiliates.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::time::Duration;
use std::str;

use agp_datapath::messages::encoder::encode_agent_from_string;
use agp_gw::config;
use tracing::info;

use mcp_client::client::{ClientCapabilities, ClientInfo, McpClient, McpClientTrait};
use mcp_client::transport::{SseTransport, Transport};
use mcp_client::McpService;


#[tokio::main]
async fn main() {
    let config_file = "../config/base/client-config.yaml";
    let id = 1;

    // try to connect to the mcpfast server
    // see mpc-client example sse.rs
    let transport = SseTransport::new("http://localhost:8000/sse", HashMap::new());
    let handle = transport.start().await.unwrap();
    let service = McpService::with_timeout(handle, Duration::from_secs(3));

    let mut client = McpClient::new(service);
    println!("MPC Client created\n");

    // Initialize
    let server_info = client
        .initialize(
            ClientInfo {
                name: "test-client".into(),
                version: "1.0.0".into(),
            },
            ClientCapabilities::default(),
        )
        .await.unwrap();
    println!("Connected to server: {server_info:?}\n");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // setup agent config
    let mut config = config::load_config(config_file).expect("failed to load configuration");
    let _guard = config.tracing.setup_tracing_subscriber();
    
    // start local agent
    // get service
    let svc_id = agp_config::component::id::ID::new_with_str("gateway/0").unwrap();
    let svc = config.services.get_mut(&svc_id).unwrap();

    // create local agent
    let agent_name = encode_agent_from_string("cisco", "default", "mpc_server", id);
    let mut rx = svc.create_agent(agent_name.clone());

    // connect to the remote gateway
    let conn_id = svc.connect(None).await.unwrap();
    info!("remote connection id = {}", conn_id);

    // wait for the connection to be established
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // send subscription
    let _ = svc.subscribe(&agent_name.agent_class, Some(id), conn_id).await;

    info!("waiting for incoming messages");
    // wait for messages
    let recv_msg = rx.recv().await.unwrap().unwrap();
    match &recv_msg.message_type.unwrap() {
        agp_datapath::pubsub::ProtoPublishType(msg) => {
            let payload = agp_datapath::messages::utils::get_payload(msg);
            let payload_str = str::from_utf8(payload).unwrap();

            info!("receive msg: {}", payload_str);

            // get message source
            let source_class;
            let source_id;
            match agp_datapath::messages::utils::process_name(&msg.source) {
                Err(e) => {
                    panic!("error parsing message {}", e);
                }
                Ok(x) => {
                    source_class = x;
                }
            }
            match agp_datapath::messages::utils::get_agent_id(&msg.source) {
                None => {
                    panic!("error parsing message: unable to get source id");
                }
                Some(x) => {
                    source_id = x;
                }
            }

            if payload_str != "tools" {
                svc.send_msg(&source_class, Some(source_id), 1, "unknown command".as_bytes().to_vec(), conn_id).await.unwrap();
            } else {
                // ask for tools to the fastmcp server
                let tools = client.list_tools(None).await.unwrap();
                info!("Available tools: {tools:?}\n");

                let reply = format!("{tools:?}");
                info!("REPLY: {}", reply);
                svc.send_msg(&source_class, Some(source_id), 1, reply.as_bytes().to_vec(), conn_id).await.unwrap();
            }
        }
        t => {
            panic!("received unexpected message: {:?}", t);
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
}
