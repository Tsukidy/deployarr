//! Finding 6, rewritten. Only the `Cargo.toml` of opus/serde-rt survived.
//!
//!   serde-rt '<stored step JSON>'
//!
//! Parses one stored step with the first draft's shapes, which hold an add-on step's config
//! as a `serde_json::Value`, serializes it again, and compares the bytes. These are the shapes
//! that haiku/serde-action copied from the first draft's sketch. The last two lines run the
//! same input through a config held as `Box<RawValue>`: first inside the first draft's
//! `#[serde(tag = "type")]` enum, then in a plain struct as the design's sketch has it.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    AddOn(AddOnStep),
    ApplyJoinProfile { stage: String },
    #[serde(untagged)]
    Builtin(BuiltinAction),
}

#[derive(Serialize, Deserialize)]
pub struct AddOnStep {
    pub addon: String,
    pub kind: String,
    pub config_version: u32,
    pub config: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BuiltinAction {
    RunCommand { command: String },
    SetVariable { name: String, value: String },
    ServerCall { send: Vec<String> },
}

/// `Box<RawValue>` inside an internally tagged enum, which buffers its content first.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum TaggedRawAction {
    AddOn { addon: String, kind: String, config_version: u32, config: Box<RawValue> },
}

/// The design's change, as in the sketch's `AddOnStep` on #86: a plain struct that reads
/// `type` itself, refuses unknown fields, and keeps the config as the JSON text it was given.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAddOnStep {
    #[serde(rename = "type")]
    tag: AddOnTag,
    addon: String,
    kind: String,
    config_version: u32,
    config: Box<RawValue>,
}

#[derive(Serialize, Deserialize)]
enum AddOnTag {
    AddOn,
}

fn round_trip<'a, T: Serialize + Deserialize<'a>>(saved: &'a str) -> String {
    match serde_json::from_str::<T>(saved) {
        Ok(v) => format!("byte-identical: {}", serde_json::to_string(&v).unwrap() == saved),
        Err(e) => format!("does not parse: {e}"),
    }
}

fn main() {
    let saved = std::env::args().nth(1).expect("usage: serde-rt '<stored step JSON>'");
    let loaded = serde_json::to_string(&serde_json::from_str::<Action>(&saved).expect("the step parses")).unwrap();
    println!("saved : {saved}");
    println!("loaded: {loaded}");
    println!("byte-identical: {}", loaded == saved);
    println!("Box<RawValue> in the #[serde(tag)] enum: {}", round_trip::<TaggedRawAction>(&saved));
    println!("Box<RawValue> in a plain struct (the design): {}", round_trip::<RawAddOnStep>(&saved));
}
