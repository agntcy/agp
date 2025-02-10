// SPDX-FileCopyrightText: Copyright (c) 2025 Cisco and/or its affiliates.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;

use agp_gw_config::component::id::{Kind, ID};
use agp_gw_config::component::Component;
use agp_gw_nop_component::{NopComponent, NopComponentBuilder, NopComponentConfig};
use serde_yaml::Error;

static TEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");

#[test]
fn test_nop_component() {
    let component_type = Kind::new("nop_component");
    assert!(component_type.is_ok());

    let id = ID::new(component_type.unwrap());
    let component = NopComponent::new(id);

    assert_eq!(component.identifier().kind().to_string(), "nop_component");
}

#[test]
fn test_nop_component_builder() {
    let kind = NopComponentBuilder::kind();
    let id = ID::new(kind.clone());
    let component = NopComponent::new(id);

    assert_eq!(component.identifier().kind().to_string(), "nop_component");
}

#[test]
fn test_nop_component_config() {
    // load file
    let mut file = File::open(format!("{}/testdata/nop.yaml", TEST_PATH)).unwrap();

    let mut contents = String::new();
    let res = file.read_to_string(&mut contents);
    assert!(res.is_ok());

    // load configuration
    let config: Result<NopComponentConfig, Error> = serde_yaml::from_str(&contents);
    assert!(config.is_ok());

    let config = config.unwrap();
    assert_eq!(config.field, "value");
}
