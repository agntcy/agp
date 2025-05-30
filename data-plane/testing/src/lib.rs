// Copyright AGNTCY Contributors (https://github.com/agntcy)
// SPDX-License-Identifier: Apache-2.0

use std::str::SplitWhitespace;

use agp_datapath::messages::{Agent, AgentType};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ParsingError {
    #[error("parsing error {0}")]
    ParsingError(String),
    #[error("end of workload")]
    EOWError,
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, Default)]
pub struct ParsedMessage {
    /// message type (SUB or PUB)
    pub msg_type: String,

    /// name used to send the publication
    pub name: Agent,

    /// publication id to add in the payload
    pub id: u64,

    /// list of possible receives for the publication
    pub receivers: Vec<u64>,
}

pub fn parse_sub(mut iter: SplitWhitespace<'_>) -> Result<ParsedMessage, ParsingError> {
    let mut subscription = ParsedMessage {
        msg_type: "SUB".to_string(),
        ..Default::default()
    };

    // this a valid subscription, skip subscription id
    iter.next();

    // get subscriber id
    match iter.next() {
        None => {
            return Err(ParsingError::ParsingError(
                "missing subscriber id".to_string(),
            ));
        }
        Some(id_str) => match id_str.parse::<u64>() {
            Ok(x) => {
                subscription.id = x;
            }
            Err(e) => {
                return Err(ParsingError::ParsingError(e.to_string()));
            }
        },
    }

    let org = iter.next().unwrap().parse::<u64>().unwrap();
    let namespace = iter.next().unwrap().parse::<u64>().unwrap();
    let agent_type_val = iter.next().unwrap().parse::<u64>().unwrap();
    let agent_id = iter.next().unwrap().parse::<u64>().unwrap();

    let a_type = AgentType::from_strings(&org.to_string(), &namespace.to_string(), &agent_type_val.to_string());
    let sub = Agent::new(a_type, agent_id);

    subscription.name = sub;
    Ok(subscription)
}

pub fn parse_pub(mut iter: SplitWhitespace<'_>) -> Result<ParsedMessage, ParsingError> {
    let mut publication = ParsedMessage {
        msg_type: "PUB".to_string(),
        ..Default::default()
    };

    // this a valid publication, get pub id
    match iter.next() {
        None => {
            // unable to parse this line
            return Err(ParsingError::ParsingError(
                "missing publication id".to_string(),
            ));
        }
        Some(x_str) => match x_str.parse::<u64>() {
            Ok(x) => publication.id = x,
            Err(e) => {
                return Err(ParsingError::ParsingError(e.to_string()));
            }
        },
    }

    // get the publication name
    let org = iter.next().unwrap().parse::<u64>().unwrap();
    let namespace = iter.next().unwrap().parse::<u64>().unwrap();
    let agent_type_val = iter.next().unwrap().parse::<u64>().unwrap();
    let agent_id = iter.next().unwrap().parse::<u64>().unwrap();

    let a_type = AgentType::from_strings(&org.to_string(), &namespace.to_string(), &agent_type_val.to_string());
    let pub_name = Agent::new(a_type, agent_id);

    // get the len of the possible receivers
    let size = match iter.next().unwrap().parse::<u64>() {
        Ok(x) => x,
        Err(e) => {
            return Err(ParsingError::ParsingError(e.to_string()));
        }
    };

    // collect the list of possible receivers
    for recv in iter {
        match recv.parse::<u64>() {
            Ok(x) => {
                publication.receivers.push(x);
            }
            Err(e) => {
                return Err(ParsingError::ParsingError(e.to_string()));
            }
        }
    }

    if size as usize != publication.receivers.len() {
        return Err(ParsingError::ParsingError(
            "missing receiver ids".to_string(),
        ));
    }

    publication.name = pub_name;
    Ok(publication)
}

pub fn parse_line(line: &str) -> Result<ParsedMessage, ParsingError> {
    let mut iter = line.split_whitespace();
    let msg_type = iter
        .next()
        .ok_or_else(|| ParsingError::ParsingError("missing type".to_string()))?
        .to_string();

    match msg_type.as_str() {
        "SUB" => parse_sub(iter),
        "PUB" => parse_pub(iter),
        _ => Err(ParsingError::ParsingError(format!("unknown type: {}", msg_type))),
    }
}
