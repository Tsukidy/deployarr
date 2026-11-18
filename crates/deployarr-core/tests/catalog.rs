//! The catalog of what add-ons contribute, as the server sends it to the web
//! UI and as `validate` reads it.

use std::collections::BTreeSet;

use deployarr_core::addon::AddOnRef;
use deployarr_core::catalog::{Availability, Catalog, requirements};
use deployarr_core::end_state::EndState;
use deployarr_core::lookup::ResolverInput;
use deployarr_core::plan::{SaveCheck, validate};
use deployarr_core::sequence::{AuthoredSequence, RunIn};

const CATALOG: &str = r#"{
  "acme.autopilot": {
    "step_kinds": {
      "register": {
        "label": "Register with Autopilot",
        "run_in": "full_os",
        "availability": "enabled",
        "config": {
          "current": 2,
          "versions": {
            "1": {"field": [{"key": "group_tag", "label": "Group tag", "kind": "template"}]},
            "2": {"field": [{"key": "group_tag", "label": "Group tag", "kind": "template", "required": true}]}
          }
        }
      }
    },
    "join_kinds": {
      "autopilot": {
        "label": "Autopilot",
        "availability": "enabled",
        "requires_end_state": ["CustomerOobe"],
        "config": {"current": 1, "versions": {"1": {}}}
      }
    }
  },
  "acme.orders": {
    "resolvers": {
      "by-order": {"label": "Order number", "availability": "disabled", "input": {"prompt": "order-number"}},
      "by-serial": {"label": "Serial number", "availability": "disabled", "input": "identity"}
    },
    "prompts": {
      "order-number": {
        "id": "order-number",
        "title": "Order number",
        "field": [{"key": "order_no", "label": "Order number", "max_len": 40}]
      }
    }
  }
}"#;

fn kind(addon: &str, kind: &str) -> AddOnRef {
    AddOnRef {
        addon: addon.parse().unwrap(),
        kind: kind.parse().unwrap(),
    }
}

#[test]
fn a_catalog_parses_and_round_trips() {
    let catalog: Catalog = serde_json::from_str(CATALOG).expect("the catalog parses");

    let register = catalog
        .step_kind(&kind("acme.autopilot", "register"))
        .expect("the step kind");
    assert_eq!(register.run_in, RunIn::FullOs);
    assert_eq!(register.availability, Availability::Enabled);
    assert_eq!(register.config.current(), 2);
    assert!(register.config.at(1).is_some());
    assert!(register.config.at(3).is_none());

    let by_order = catalog
        .resolver(&kind("acme.orders", "by-order"))
        .expect("the resolver");
    assert_eq!(
        by_order.input,
        ResolverInput::Prompt("order-number".parse().unwrap())
    );
    let prompt = catalog
        .prompt(&kind("acme.orders", "order-number"))
        .expect("the prompt");
    assert_eq!(prompt.title(), "Order number");
    assert!(
        catalog
            .prompt(&kind("acme.autopilot", "order-number"))
            .is_none()
    );

    let json = serde_json::to_string(&catalog).expect("a catalog serializes");
    assert_eq!(
        serde_json::from_str::<Catalog>(&json).expect("the JSON parses"),
        catalog
    );
}

#[test]
fn a_catalog_refuses_contributions_that_do_not_fit_together() {
    for (change, message) in [
        (
            CATALOG.replace("\"current\": 2", "\"current\": 3"),
            "no schema for the current config version 3",
        ),
        (
            CATALOG.replace(
                "\"input\": {\"prompt\": \"order-number\"}",
                "\"input\": {\"prompt\": \"order-no\"}",
            ),
            "reads the prompt `order-no`, which the add-on does not declare",
        ),
        (
            CATALOG.replace("\"id\": \"order-number\"", "\"id\": \"order-no\""),
            "the prompt under `order-number` has the ID `order-no`",
        ),
        (
            CATALOG.replace("\"step_kinds\"", "\"steps\""),
            "unknown field `steps`",
        ),
        (
            CATALOG.replace("\"availability\": \"enabled\"", "\"availability\": \"on\""),
            "unknown variant `on`",
        ),
    ] {
        let error = serde_json::from_str::<Catalog>(&change).expect_err(message);
        assert!(error.to_string().contains(message), "{message}: {error}");
    }
}

#[test]
fn validate_reads_the_step_kinds_of_the_catalog() {
    let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
    let document = |addon: &str| {
        format!(
            concat!(
                r#"{{"schema_version":1,"id":"W11","version":1,"name":"W11","default_vars":[],"#,
                r#""root":{{"meta":{{"id":"00000000-0000-4000-8000-000000000001","name":"Root","enabled":true,"continue_on_error":false,"condition":null}},"children":["#,
                r#"{{"step":{{"meta":{{"id":"00000000-0000-4000-8000-000000000002","name":"Register","enabled":true,"continue_on_error":false,"condition":null}},"#,
                r#""run_in":"full_os","action":{{"add_on":{{"addon":"{addon}","kind":"register","config_version":2,"config":{{"group_tag":"lab"}}}}}}}}}}]}}}}"#
            ),
            addon = addon
        )
    };

    let known = AuthoredSequence::from_json(&document("acme.autopilot")).expect("it parses");
    assert_eq!(validate(&known, &catalog), []);

    let unknown = AuthoredSequence::from_json(&document("acme.gone")).expect("it parses");
    let findings = validate(&unknown, &catalog);
    assert!(matches!(
        findings.as_slice(),
        [finding] if finding.check == SaveCheck::UnknownStepKind { kind: kind("acme.gone", "register") }
    ));
}

/// Autopilot only customizes OOBE, so its join kind needs `CustomerOobe`. A
/// join kind that names no end state allows both.
#[test]
fn a_join_kind_names_the_end_states_it_needs() {
    let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
    let autopilot = catalog
        .join_kind(&kind("acme.autopilot", "autopilot"))
        .expect("the join kind");
    assert_eq!(autopilot.requires_end_state, [EndState::CustomerOobe]);
    assert!(autopilot.allows_end_state(EndState::CustomerOobe));
    assert!(!autopilot.allows_end_state(EndState::ReadyToUse));

    let any: Catalog =
        serde_json::from_str(&CATALOG.replace(r#""requires_end_state": ["CustomerOobe"],"#, ""))
            .expect("the key is optional");
    let any = any
        .join_kind(&kind("acme.autopilot", "autopilot"))
        .expect("the join kind");
    assert!(any.requires_end_state.is_empty());
    assert!(any.allows_end_state(EndState::ReadyToUse));
    assert!(any.allows_end_state(EndState::CustomerOobe));

    let error =
        serde_json::from_str::<Catalog>(&CATALOG.replace("[\"CustomerOobe\"]", "[\"Oobe\"]"))
            .expect_err("an unknown end state");
    assert!(
        error.to_string().contains("unknown variant `Oobe`"),
        "{error}"
    );
}

/// Every add-on step counts once, a disabled one and an unknown kind too.
/// A built-in step and a join slot name no add-on.
#[test]
fn requirements_lists_every_add_on_step_kind_once() {
    let meta = |n: u32, enabled: bool| {
        format!(
            r#"{{"id":"00000000-0000-4000-8000-{n:012}","name":"N{n}","enabled":{enabled},"continue_on_error":false,"condition":null}}"#
        )
    };
    let add_on = |n: u32, addon: &str, kind: &str| {
        format!(
            r#"{{"step":{{"meta":{},"run_in":"full_os","action":{{"add_on":{{"addon":"{addon}","kind":"{kind}","config_version":1,"config":{{}}}}}}}}}}"#,
            meta(n, true)
        )
    };
    let children = [
        add_on(2, "acme.autopilot", "register"),
        format!(
            r#"{{"group":{{"meta":{},"children":[{}]}}}}"#,
            meta(3, false),
            add_on(4, "acme.gone", "export")
        ),
        add_on(5, "acme.autopilot", "register"),
        format!(
            r#"{{"step":{{"meta":{},"run_in":"either","action":{{"set_variable":{{"name":"GroupTag","value":"lab"}}}}}}}}"#,
            meta(6, true)
        ),
        format!(
            r#"{{"step":{{"meta":{},"run_in":"full_os","action":{{"apply_join_profile":{{"stage":"full_os"}}}}}}}}"#,
            meta(7, true)
        ),
    ]
    .join(",");
    let sequence = AuthoredSequence::from_json(&format!(
        r#"{{"schema_version":1,"id":"W11","version":1,"name":"W11","default_vars":[],"root":{{"meta":{},"children":[{children}]}}}}"#,
        meta(1, true)
    ))
    .expect("it parses");
    assert_eq!(
        requirements(&sequence),
        BTreeSet::from([
            kind("acme.autopilot", "register"),
            kind("acme.gone", "export"),
        ])
    );
}
