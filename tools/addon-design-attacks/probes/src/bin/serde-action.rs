// Finding 23. The text of haiku/serde-action/src/main.rs as recovered on #82, unchanged
// except for this header.
//
// The sketch's Action / BuiltinAction shapes, copied from
// scratchpad/arena/synthesis/sketch/core/addon.rs (fields trimmed to strings).
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
pub enum Action {
    AddOn(AddOnStep),
    ApplyJoinProfile { stage: String },
    #[serde(untagged)]
    Builtin(BuiltinAction),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct AddOnStep {
    pub addon: String,
    pub kind: String,
    pub config_version: u32,
    pub config: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
pub enum BuiltinAction {
    RunCommand { command: String },
    SetVariable { name: String, value: String },
    ServerCall { send: Vec<String> },
}

fn show(label: &str, json: &str) {
    match serde_json::from_str::<Action>(json) {
        Ok(a) => {
            let back = serde_json::to_string(&a).unwrap();
            println!("{label}: parsed as {a:?}\n    reserialized: {back}");
        }
        Err(e) => println!("{label}: ERROR {e}"),
    }
}

fn main() {
    show("1 builtin RunCommand", r#"{"type":"RunCommand","command":"x"}"#);
    show("2 builtin ServerCall", r#"{"type":"ServerCall","send":["a"]}"#);
    show("3 addon step", r#"{"type":"AddOn","addon":"deployarr.sql-order-lookup","kind":"lookup","config_version":1,"config":{"rules":[]}}"#);
    show("4 addon step missing config_version", r#"{"type":"AddOn","addon":"deployarr.x","kind":"k","config":{}}"#);
    show("5 unknown future tag", r#"{"type":"FutureStep","x":1}"#);
    show("6 unknown tag with addon-looking fields", r#"{"type":"Restart","into":"Reboot"}"#);
    show("7 builtin SetVariable with extra field", r#"{"type":"SetVariable","name":"a","value":"b","sneaky":true}"#);
    show("8 join profile", r#"{"type":"ApplyJoinProfile","stage":"OfflineImage"}"#);
    // Round trip of a sequence step holding an unknown add-on kind: must be unchanged.
    let s = r#"{"type":"AddOn","addon":"deployarr.entra-join","kind":"join-bulk","config_version":3,"config":{"tenant":"t","secret_ref":"s1"}}"#;
    let a: Action = serde_json::from_str(s).unwrap();
    let back: serde_json::Value = serde_json::to_value(&a).unwrap();
    let orig: serde_json::Value = serde_json::from_str(s).unwrap();
    println!("9 addon round trip unchanged: {}", back == orig);
}
