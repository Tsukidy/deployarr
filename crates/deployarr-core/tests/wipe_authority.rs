//! `WipeAuthority`'s constructors, `UsableIdentity`, and the lookup chain's
//! shapes.

use std::collections::BTreeMap;

use deployarr_core::addon::{AddOnRef, KindId};
use deployarr_core::lookup::{
    AuthorityError, IdentityFact, IdentityKind, LookupInput, PromptAnswers, Resolver,
    ResolverInput, UsableIdentity, WipeAuthority,
};
use deployarr_core::sequence::TaskSequenceId;
use serde_json::json;

fn resolver(kind: &str) -> AddOnRef {
    AddOnRef {
        addon: "deployarr.sql-order-lookup"
            .parse()
            .expect("a valid add-on ID"),
        kind: kind.parse().expect("a valid kind ID"),
    }
}

fn fact(kind: IdentityKind, value: &str) -> IdentityFact {
    IdentityFact {
        kind,
        value: value.to_owned(),
    }
}

fn prompt_id(id: &str) -> KindId {
    id.parse().expect("a valid prompt ID")
}

/// A PC with a real baseboard serial and a placeholder system serial.
fn identity() -> LookupInput {
    LookupInput::Identity(
        UsableIdentity::from_gathered(&[
            fact(IdentityKind::SystemSerial, "To Be Filled By O.E.M."),
            fact(IdentityKind::BaseboardSerial, "MB-2231-0042"),
        ])
        .expect("the baseboard serial is usable"),
    )
}

/// The answers to a prompt of `addon`.
fn answers_from(addon: &str, prompt: &str, answer: &str) -> LookupInput {
    LookupInput::Prompt(
        PromptAnswers::new(
            AddOnRef {
                addon: addon.parse().expect("a valid add-on ID"),
                kind: prompt_id(prompt),
            },
            BTreeMap::from([("order_no".to_owned(), answer.to_owned())]),
        )
        .expect("the answer is not blank"),
    )
}

/// The answers to a prompt of the resolvers' add-on.
fn answers(prompt: &str, answer: &str) -> LookupInput {
    answers_from("deployarr.sql-order-lookup", prompt, answer)
}

#[test]
fn an_identity_match_on_a_sent_fact_is_authority() {
    let by = resolver("by-identity");

    let authority = WipeAuthority::from_addon_match(
        &by,
        &ResolverInput::Identity,
        &identity(),
        Some(IdentityKind::BaseboardSerial),
    )
    .expect("the match gives authority");

    let WipeAuthority::IdentityMatch(found) = authority else {
        panic!("expected an identity match, found {authority:?}");
    };
    assert_eq!(found.by(), &by);
    assert_eq!(
        found.facts(),
        [fact(IdentityKind::BaseboardSerial, "MB-2231-0042")]
    );
}

#[test]
fn a_typed_answer_is_authority() {
    let by = resolver("by-order-number");

    let authority = WipeAuthority::from_addon_match(
        &by,
        &ResolverInput::Prompt(prompt_id("order-number")),
        &answers("order-number", "SO-1001"),
        None,
    )
    .expect("the match gives authority");

    let WipeAuthority::TechnicianEntry(entry) = authority else {
        panic!("expected a technician entry, found {authority:?}");
    };
    assert_eq!(entry.by(), &by);
    assert_eq!(entry.prompt(), &prompt_id("order-number"));
}

#[test]
fn an_identity_match_with_no_matched_on_is_an_error() {
    assert_eq!(
        WipeAuthority::from_addon_match(
            &resolver("by-identity"),
            &ResolverInput::Identity,
            &identity(),
            None,
        ),
        Err(AuthorityError::NoKindNamed)
    );
}

#[test]
fn a_kind_that_the_server_did_not_send_is_an_error() {
    for kind in [IdentityKind::SystemSerial, IdentityKind::Mac] {
        assert_eq!(
            WipeAuthority::from_addon_match(
                &resolver("by-identity"),
                &ResolverInput::Identity,
                &identity(),
                Some(kind),
            ),
            Err(AuthorityError::KindNotSent(kind))
        );
    }
}

#[test]
fn a_prompt_resolvers_match_that_names_a_kind_is_an_error() {
    assert_eq!(
        WipeAuthority::from_addon_match(
            &resolver("by-order-number"),
            &ResolverInput::Prompt(prompt_id("order-number")),
            &answers("order-number", "SO-1001"),
            Some(IdentityKind::BaseboardSerial),
        ),
        Err(AuthorityError::KindNamedForPrompt(
            IdentityKind::BaseboardSerial
        ))
    );
}

#[test]
fn input_that_does_not_fit_the_declaration_is_an_error() {
    let cases = [
        (
            ResolverInput::Identity,
            answers("order-number", "SO-1001"),
            Some(IdentityKind::BaseboardSerial),
        ),
        (
            ResolverInput::Prompt(prompt_id("order-number")),
            identity(),
            None,
        ),
        (
            ResolverInput::Prompt(prompt_id("order-number")),
            answers("asset-tag", "A-77"),
            None,
        ),
    ];

    for (declared, sent, matched_on) in cases {
        assert_eq!(
            WipeAuthority::from_addon_match(&resolver("r"), &declared, &sent, matched_on),
            Err(AuthorityError::InputDoesNotMatchDeclaration),
            "{declared:?} with {sent:?}"
        );
    }
}

#[test]
fn placeholder_serials_alone_give_no_usable_identity() {
    let gathered = [
        fact(IdentityKind::SystemSerial, "To Be Filled By O.E.M."),
        fact(IdentityKind::BaseboardSerial, "Default string"),
        fact(IdentityKind::ChassisSerial, "  system serial number "),
        fact(IdentityKind::ChassisSerial, "Chassis Serial Number"),
        fact(IdentityKind::BaseboardSerial, ""),
        fact(IdentityKind::SystemSerial, "0000000"),
        fact(IdentityKind::Uuid, "00000000-0000-0000-0000-000000000000"),
        fact(IdentityKind::Uuid, "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"),
        fact(IdentityKind::Mac, "00:00:00:00:00:00"),
    ];

    assert_eq!(UsableIdentity::from_gathered(&gathered), None);
    assert_eq!(UsableIdentity::from_gathered(&[]), None);
}

#[test]
fn a_usable_identity_keeps_only_the_real_facts() {
    let real = [
        fact(IdentityKind::BaseboardSerial, "MB-2231-0042"),
        fact(IdentityKind::Uuid, "4c4c4544-0042-3510-8051-b4c04f4e3332"),
        fact(IdentityKind::Mac, "00:15:5D:01:02:03"),
        fact(IdentityKind::SystemSerial, "10"),
    ];
    let mut gathered = vec![fact(IdentityKind::SystemSerial, "Default string")];
    gathered.extend(real.iter().cloned());

    let identity = UsableIdentity::from_gathered(&gathered).expect("real facts are left");

    assert_eq!(identity.facts(), real);
    assert_eq!(
        identity.of_kind(IdentityKind::Mac).collect::<Vec<_>>(),
        [&real[2]]
    );
}

#[test]
fn answers_to_the_same_prompt_id_of_another_add_on_are_an_error() {
    assert_eq!(
        WipeAuthority::from_addon_match(
            &resolver("by-order-number"),
            &ResolverInput::Prompt(prompt_id("order-number")),
            &answers_from("acme.orders", "order-number", "SO-1001"),
            None,
        ),
        Err(AuthorityError::InputDoesNotMatchDeclaration)
    );
}

#[test]
fn blank_answers_are_no_typed_answer() {
    let blank = BTreeMap::from([("order_no".to_owned(), "   ".to_owned())]);
    let prompt = resolver("order-number");

    assert_eq!(PromptAnswers::new(prompt.clone(), blank), None);
    assert_eq!(PromptAnswers::new(prompt, BTreeMap::new()), None);
}

#[test]
fn debug_output_never_holds_an_answer() {
    let sent = answers("order-number", "SECRET-ANSWER-7731");

    let printed = format!("{sent:?}");

    assert!(!printed.contains("SECRET-ANSWER-7731"), "{printed}");
    assert!(printed.contains("order_no"), "{printed}");
}

#[test]
fn a_usable_identity_holds_trimmed_values() {
    let identity =
        UsableIdentity::from_gathered(&[fact(IdentityKind::BaseboardSerial, "  MB-2231-0042   ")])
            .expect("the serial is usable");

    assert_eq!(
        identity.facts(),
        [fact(IdentityKind::BaseboardSerial, "MB-2231-0042")]
    );
}

#[test]
fn identity_kinds_and_resolvers_have_their_json_names() {
    let kinds = [
        (IdentityKind::SystemSerial, "system_serial"),
        (IdentityKind::BaseboardSerial, "baseboard_serial"),
        (IdentityKind::ChassisSerial, "chassis_serial"),
        (IdentityKind::Uuid, "uuid"),
        (IdentityKind::Mac, "mac"),
    ];
    for (kind, name) in kinds {
        assert_eq!(
            serde_json::to_value(kind).expect("it serializes"),
            json!(name)
        );
        assert_eq!(kind.to_string(), name);
    }

    let chain = json!([
        "pre_staged",
        { "add_on": { "addon": "deployarr.sql-order-lookup", "kind": "by-identity" } },
        "manual_list",
    ]);
    let parsed: Vec<Resolver> = serde_json::from_value(chain.clone()).expect("the chain parses");
    assert_eq!(
        parsed,
        [
            Resolver::PreStaged,
            Resolver::AddOn(resolver("by-identity")),
            Resolver::ManualList,
        ]
    );
    assert_eq!(serde_json::to_value(&parsed).expect("it serializes"), chain);
}

/// A PC with a placeholder system serial, a real baseboard serial and UUID,
/// and two network adapters.
fn pre_staged_identity() -> UsableIdentity {
    UsableIdentity::from_gathered(&[
        fact(IdentityKind::SystemSerial, "To Be Filled By O.E.M."),
        fact(IdentityKind::BaseboardSerial, "MB-2231-0042"),
        fact(IdentityKind::Mac, "00:15:5D:01:02:03"),
        fact(IdentityKind::Uuid, "4c4c4544-0042-3510-8051-b4c04f4e3332"),
        fact(IdentityKind::Mac, "00:15:5D:01:02:04"),
    ])
    .expect("real facts are left")
}

fn task_sequence(id: &str) -> TaskSequenceId {
    id.parse().expect("a valid task sequence ID")
}

#[test]
fn a_pre_staged_match_on_a_usable_kind_holds_exactly_its_facts() {
    let cases = [
        (
            IdentityKind::BaseboardSerial,
            vec![fact(IdentityKind::BaseboardSerial, "MB-2231-0042")],
        ),
        (
            IdentityKind::Mac,
            vec![
                fact(IdentityKind::Mac, "00:15:5D:01:02:03"),
                fact(IdentityKind::Mac, "00:15:5D:01:02:04"),
            ],
        ),
    ];

    for (kind, expected) in cases {
        let authority = WipeAuthority::from_pre_staged_match(&pre_staged_identity(), kind)
            .expect("the match gives authority");

        let WipeAuthority::PreStagedMatch(found) = authority else {
            panic!("expected a pre-staged match, found {authority:?}");
        };
        assert_eq!(found.facts(), expected, "{kind}");
    }
}

#[test]
fn a_pre_staged_match_on_a_kind_the_identity_lacks_is_an_error() {
    // The system serial was a placeholder, and the agent gathered no chassis
    // serial.
    for kind in [IdentityKind::SystemSerial, IdentityKind::ChassisSerial] {
        assert_eq!(
            WipeAuthority::from_pre_staged_match(&pre_staged_identity(), kind),
            Err(AuthorityError::KindNotUsable(kind))
        );
    }
}

#[test]
fn a_manual_pick_of_an_offered_task_sequence_is_authority() {
    let offered = [task_sequence("W11-PRO-STD"), task_sequence("W11-HOME-STD")];

    let authority = WipeAuthority::from_manual_pick(&offered, &task_sequence("W11-HOME-STD"))
        .expect("the pick gives authority");

    let WipeAuthority::ManualPick(pick) = authority else {
        panic!("expected a manual pick, found {authority:?}");
    };
    assert_eq!(pick.task_sequence(), &task_sequence("W11-HOME-STD"));
}

#[test]
fn a_manual_pick_that_was_not_offered_is_an_error() {
    let picked = task_sequence("W10-PRO-STD");
    let offered = [task_sequence("W11-PRO-STD"), task_sequence("W11-HOME-STD")];

    for offered in [&offered[..], &[]] {
        assert_eq!(
            WipeAuthority::from_manual_pick(offered, &picked),
            Err(AuthorityError::PickNotOffered(picked.clone())),
            "offered {offered:?}"
        );
    }
}

#[test]
fn pre_staged_and_manual_pick_errors_name_what_failed() {
    assert_eq!(
        AuthorityError::KindNotUsable(IdentityKind::ChassisSerial).to_string(),
        "the pre-staged record matched on `chassis_serial`, and the PC has no usable fact of that kind"
    );
    assert_eq!(
        AuthorityError::PickNotOffered(task_sequence("W10-PRO-STD")).to_string(),
        "the technician picked `W10-PRO-STD`, and the manual list that the server offered does not hold it"
    );
}
