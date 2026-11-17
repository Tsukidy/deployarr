//! The pure planner of add-on changes. Each refusal has a table row, and a
//! waiting operation has its three cases from item 14 of
//! `docs/research/addon-design-attacks-wasmtime-50.0.1.md`: Force for the same
//! change takes it over, `cancel` abandons it, and any other change is refused
//! with the waiting operation named.

use std::ops::RangeInclusive;

use deployarr_core::addon::AddOnId;
use deployarr_core::lifecycle::{
    Approval, Change, Commit, Effect, Facts, Installed, OpKind, OpPhase, OpenOperation, Package,
    PackageHash, Plan, PublicKey, Publisher, Refusal, Refusals, Registry, SequenceVersion, Signer,
    Step, Trust, Usage, WhenBusy, plan,
};
use deployarr_core::non_empty::NonEmpty;
use semver::Version;

const ACME: PublicKey = PublicKey([1; 32]);
const OTHER: PublicKey = PublicKey([2; 32]);
/// A key that the trust bundle does not list.
const STRANGER: PublicKey = PublicKey([3; 32]);

fn hash(n: u8) -> PackageHash {
    PackageHash([n; 32])
}

fn id() -> AddOnId {
    "acme.orders".parse().unwrap()
}

fn version(text: &str) -> Version {
    text.parse().unwrap()
}

/// Package `n` of `acme.orders`, signed by Acme, at API level 1 and data
/// version 1.
fn package(n: u8, text: &str) -> Package {
    Package {
        hash: hash(n),
        id: id(),
        version: version(text),
        api_level: 1,
        data_version: 1,
        signer: Signer::Key(ACME),
    }
}

/// `acme.orders` 1.0.0, package 1, installed signed and enabled.
fn installed() -> Installed {
    Installed {
        active: package(1, "1.0.0"),
        previous: None,
        enabled: true,
        uninstalled: false,
        publisher: Some("Acme".into()),
        store_data_version: 1,
        open: None,
    }
}

fn publisher(name: &str, key: PublicKey, scope: &str) -> Publisher {
    Publisher {
        name: name.into(),
        key,
        scopes: NonEmpty::new(scope.parse().unwrap()),
        local: false,
    }
}

/// The server's state and the facts around one change.
struct World {
    registry: Registry,
    serves: RangeInclusive<u32>,
    trust: Trust,
    allow_unsigned: bool,
    usage: Usage,
}

impl World {
    /// No add-on installed. Acme may sign `acme.*`, and Other may sign
    /// `other.*`. The server serves levels 0 and 1.
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            serves: 0..=1,
            trust: Trust {
                publishers: vec![
                    publisher("Acme", ACME, "acme.*"),
                    publisher("Other", OTHER, "other.*"),
                ],
                ..Trust::default()
            },
            allow_unsigned: false,
            usage: Usage::default(),
        }
    }

    fn with(installed: Installed) -> Self {
        let mut world = Self::new();
        world.registry.add_ons.insert(id(), installed);
        world
    }

    fn plan(&self, change: &Change) -> Result<Plan, Refusals> {
        let facts = Facts {
            serves: &self.serves,
            trust: &self.trust,
            allow_unsigned: self.allow_unsigned,
            usage: &self.usage,
        };
        plan(&self.registry, change, &facts)
    }

    fn refusals(&self, change: &Change) -> Vec<Refusal> {
        match self.plan(change) {
            Ok(plan) => panic!("expected a refusal, got {plan:?}"),
            Err(refusals) => refusals.as_slice().to_vec(),
        }
    }
}

fn apply(package: Package) -> Change {
    Change::Apply {
        package,
        approval: Approval::default(),
    }
}

fn apply_with(package: Package, approval: Approval) -> Change {
    Change::Apply { package, approval }
}

fn forced() -> Approval {
    Approval {
        when_busy: WhenBusy::Force,
        ..Approval::default()
    }
}

fn used_by_a_sequence() -> Usage {
    Usage {
        task_sequences: vec![SequenceVersion {
            id: "W11-PRO".parse().unwrap(),
            version: 3,
        }],
        ..Usage::default()
    }
}

/// An upgrade to package 2 that waits for two running deployments.
fn waiting() -> OpenOperation {
    OpenOperation {
        kind: OpKind::Apply { package: hash(2) },
        phase: OpPhase::Approved,
    }
}

fn with_open(open: OpenOperation) -> World {
    let mut world = World::with(Installed {
        open: Some(open),
        ..installed()
    });
    world.usage.running_deployments = 2;
    world
}

// ------------------------------------------------------- the refusals of M0

#[test]
fn every_refusal_that_the_roadmap_names() {
    let incompatible = World::new();
    let revoked_key = {
        let mut world = World::new();
        world.trust.revoked_keys.insert(ACME);
        world
    };
    let in_use = {
        let mut world = World::with(installed());
        world.usage = used_by_a_sequence();
        world
    };
    let open = OpenOperation {
        kind: OpKind::Apply { package: hash(3) },
        phase: OpPhase::Preparing,
    };
    let rows: Vec<(&str, World, Change, Vec<Refusal>)> = vec![
        (
            "an incompatible level",
            incompatible,
            apply(Package {
                api_level: 2,
                ..package(1, "1.0.0")
            }),
            vec![Refusal::IncompatibleLevel {
                needs: 2,
                serves: 0..=1,
            }],
        ),
        (
            "a revoked key",
            revoked_key,
            apply(package(1, "1.0.0")),
            vec![Refusal::RevokedKey { key: ACME }],
        ),
        (
            "a key outside its scope",
            World::new(),
            apply(Package {
                signer: Signer::Key(OTHER),
                ..package(1, "1.0.0")
            }),
            vec![Refusal::KeyOutOfScope {
                publisher: "Other".into(),
                id: id(),
            }],
        ),
        (
            "a downgrade without confirmation",
            World::with(installed()),
            apply(package(2, "0.9.0")),
            vec![Refusal::Downgrade {
                from: version("1.0.0"),
                to: version("0.9.0"),
            }],
        ),
        (
            "an uninstall while a sequence uses the add-on",
            in_use,
            Change::Uninstall { addon: id() },
            vec![Refusal::InUse(used_by_a_sequence())],
        ),
        (
            "a second open operation",
            World::with(Installed {
                open: Some(open.clone()),
                ..installed()
            }),
            apply(package(2, "1.1.0")),
            vec![Refusal::Busy { open }],
        ),
    ];
    for (name, world, change, want) in rows {
        assert_eq!(world.refusals(&change), want, "{name}");
    }
}

// --------------------------------------------------- a waiting operation

#[test]
fn force_for_the_same_package_takes_the_waiting_operation_over() {
    let world = with_open(waiting());
    let plan = world
        .plan(&apply_with(package(2, "1.1.0"), forced()))
        .expect("Force takes the waiting operation over");
    assert_eq!(plan.kind, Some(OpKind::Apply { package: hash(2) }));
    assert_eq!(
        plan.steps,
        [
            Step::TakeOver,
            Step::Prepare(hash(2)),
            Step::Check(hash(2)),
            Step::Commit(Commit::Packages {
                active: hash(2),
                previous: Some(hash(1)),
                publisher: Some("Acme".into()),
                enabled: true,
            }),
        ]
    );
    assert_eq!(
        plan.effects,
        [
            Effect::TakesOver,
            Effect::Upgrades {
                from: version("1.0.0"),
                to: version("1.1.0"),
            },
            Effect::Forced {
                running_deployments: 2
            },
        ]
    );
}

#[test]
fn cancel_abandons_the_waiting_operation() {
    let world = with_open(waiting());
    let plan = world
        .plan(&Change::Cancel { addon: id() })
        .expect("cancel abandons a waiting operation");
    assert_eq!(
        plan,
        Plan {
            addon: id(),
            kind: None,
            steps: vec![Step::Abandon],
            effects: vec![Effect::Abandons {
                waiting: waiting().kind
            }],
        }
    );
}

#[test]
fn any_other_change_is_refused_with_the_waiting_operation_named() {
    let world = with_open(waiting());
    let others = [
        ("the same package without Force", apply(package(2, "1.1.0"))),
        (
            "the same package with Refuse",
            apply_with(
                package(2, "1.1.0"),
                Approval {
                    when_busy: WhenBusy::Refuse,
                    ..Approval::default()
                },
            ),
        ),
        (
            "another package with Force",
            apply_with(package(3, "1.2.0"), forced()),
        ),
        (
            "a roll back with Force",
            Change::RollBack {
                addon: id(),
                when_busy: WhenBusy::Force,
            },
        ),
        ("an uninstall", Change::Uninstall { addon: id() }),
        (
            "a disable with Force",
            Change::Disable {
                addon: id(),
                when_busy: WhenBusy::Force,
            },
        ),
        ("an enable", Change::Enable { addon: id() }),
        ("a purge", Change::Purge { addon: id() }),
    ];
    for (name, change) in others {
        let refusals = world.refusals(&change);
        assert_eq!(refusals, [Refusal::Busy { open: waiting() }], "{name}");
        let message = refusals[0].to_string();
        assert!(
            message.contains(&format!("the change to package {}", hash(2)))
                && message.contains("waits for deployments"),
            "{name}: {message}"
        );
    }
}

#[test]
fn cancel_refuses_an_operation_that_started() {
    let started = OpenOperation {
        phase: OpPhase::Migrating,
        ..waiting()
    };
    let world = with_open(started.clone());
    assert_eq!(
        world.refusals(&Change::Cancel { addon: id() }),
        [Refusal::Started {
            open: started.clone()
        }]
    );
    assert_eq!(
        world.refusals(&apply_with(package(2, "1.1.0"), forced())),
        [Refusal::Busy { open: started }],
        "Force takes over only a waiting operation"
    );
}

#[test]
fn cancel_needs_a_waiting_operation() {
    assert_eq!(
        World::with(installed()).refusals(&Change::Cancel { addon: id() }),
        [Refusal::NothingToCancel]
    );
    assert_eq!(
        World::new().refusals(&Change::Cancel { addon: id() }),
        [Refusal::UnknownAddOn]
    );
}

// --------------------------------------------------------------- plans

#[test]
fn an_install_migrates_a_new_store_from_data_version_0() {
    let plan = World::new()
        .plan(&apply(package(1, "1.0.0")))
        .expect("a signed package installs");
    assert_eq!(
        plan.steps,
        [
            Step::Approve,
            Step::Prepare(hash(1)),
            Step::MigrateData { from: 0, to: 1 },
            Step::Check(hash(1)),
            Step::Commit(Commit::Packages {
                active: hash(1),
                previous: None,
                publisher: Some("Acme".into()),
                enabled: true,
            }),
        ]
    );
    assert_eq!(
        plan.effects,
        [
            Effect::Installs {
                version: version("1.0.0")
            },
            Effect::MigratesData { from: 0, to: 1 },
        ]
    );
}

#[test]
fn an_upgrade_keeps_the_old_package_as_previous() {
    let plan = World::with(installed())
        .plan(&apply(Package {
            data_version: 2,
            ..package(2, "1.1.0")
        }))
        .expect("an upgrade");
    assert_eq!(
        plan.steps,
        [
            Step::Approve,
            Step::Prepare(hash(2)),
            Step::MigrateData { from: 1, to: 2 },
            Step::Check(hash(2)),
            Step::Commit(Commit::Packages {
                active: hash(2),
                previous: Some(hash(1)),
                publisher: Some("Acme".into()),
                enabled: true,
            }),
        ]
    );
}

#[test]
fn a_confirmed_downgrade_goes_ahead() {
    let plan = World::with(installed())
        .plan(&apply_with(
            package(2, "0.9.0"),
            Approval {
                allow_downgrade: true,
                ..Approval::default()
            },
        ))
        .expect("a confirmed downgrade");
    assert_eq!(
        plan.effects,
        [Effect::Downgrades {
            from: version("1.0.0"),
            to: version("0.9.0"),
        }]
    );
}

#[test]
fn an_upgrade_waits_for_running_deployments_by_default() {
    let mut world = World::with(installed());
    world.usage.running_deployments = 2;
    let plan = world.plan(&apply(package(2, "1.1.0"))).expect("it waits");
    assert_eq!(plan.steps[..2], [Step::Approve, Step::WaitForDeployments]);
    assert!(
        plan.effects
            .contains(&Effect::WaitsForDeployments { count: 2 })
    );

    let refuse = Approval {
        when_busy: WhenBusy::Refuse,
        ..Approval::default()
    };
    assert_eq!(
        world.refusals(&apply_with(package(2, "1.1.0"), refuse)),
        [Refusal::RunningDeployments { count: 2 }]
    );
}

#[test]
fn a_roll_back_restores_the_snapshot_of_an_older_data_version() {
    let world = World::with(Installed {
        active: Package {
            data_version: 2,
            ..package(2, "1.1.0")
        },
        previous: Some(package(1, "1.0.0")),
        store_data_version: 2,
        ..installed()
    });
    let plan = world
        .plan(&Change::RollBack {
            addon: id(),
            when_busy: WhenBusy::Wait,
        })
        .expect("a roll back");
    assert_eq!(plan.kind, Some(OpKind::RollBack { to: hash(1) }));
    assert_eq!(
        plan.steps,
        [
            Step::Approve,
            Step::Prepare(hash(1)),
            Step::RestoreSnapshot { to: 1 },
            Step::Check(hash(1)),
            Step::Commit(Commit::Packages {
                active: hash(1),
                previous: Some(hash(2)),
                publisher: Some("Acme".into()),
                enabled: true,
            }),
        ]
    );
    assert_eq!(
        plan.effects,
        [
            Effect::RollsBack {
                from: version("1.1.0"),
                to: version("1.0.0"),
            },
            Effect::RestoresSnapshot { from: 2, to: 1 },
        ]
    );
}

#[test]
fn enable_disable_uninstall_and_purge_each_commit_one_change() {
    let disabled = World::with(Installed {
        enabled: false,
        ..installed()
    });
    let uninstalled = World::with(Installed {
        uninstalled: true,
        ..installed()
    });
    for (world, change, commit, effect) in [
        (
            &disabled,
            Change::Enable { addon: id() },
            Commit::Enabled(true),
            Effect::Enables,
        ),
        (
            &World::with(installed()),
            Change::Disable {
                addon: id(),
                when_busy: WhenBusy::Wait,
            },
            Commit::Enabled(false),
            Effect::Disables,
        ),
        (
            &disabled,
            Change::Uninstall { addon: id() },
            Commit::Uninstalled,
            Effect::Uninstalls,
        ),
        (
            &uninstalled,
            Change::Purge { addon: id() },
            Commit::Purged,
            Effect::Purges,
        ),
    ] {
        let plan = world.plan(&change).expect("the change is planned");
        assert_eq!(
            plan.steps,
            [Step::Approve, Step::Commit(commit)],
            "{change:?}"
        );
        assert_eq!(plan.effects, [effect], "{change:?}");
    }
}

#[test]
fn a_reinstall_after_an_uninstall_comes_back_enabled_on_the_kept_store() {
    let world = World::with(Installed {
        enabled: false,
        uninstalled: true,
        store_data_version: 3,
        ..installed()
    });
    assert_eq!(
        world.refusals(&apply(package(2, "1.1.0"))),
        [Refusal::DataTooNew {
            store: 3,
            package: 1,
        }]
    );
    let plan = world
        .plan(&apply(Package {
            data_version: 3,
            ..package(2, "1.1.0")
        }))
        .expect("a reinstall");
    assert_eq!(
        plan.steps.last(),
        Some(&Step::Commit(Commit::Packages {
            active: hash(2),
            previous: Some(hash(1)),
            publisher: Some("Acme".into()),
            enabled: true,
        }))
    );
}

// ----------------------------------------------------- more refusals

#[test]
fn more_refusals() {
    let revoked_package = || {
        let mut world = World::new();
        world.trust.revoked_packages.insert(hash(1));
        world
    };
    let unsigned_allowed = || {
        let mut world = World::new();
        world.allow_unsigned = true;
        world
    };
    let busy = || {
        let mut world = World::with(installed());
        world.usage.running_deployments = 1;
        world
    };
    let old_level = || {
        let mut world = World::with(Installed {
            previous: Some(Package {
                api_level: 0,
                ..package(0, "0.9.0")
            }),
            ..installed()
        });
        world.serves = 1..=1;
        world
    };
    let revoked_previous = || {
        let mut world = World::with(Installed {
            previous: Some(package(0, "0.9.0")),
            ..installed()
        });
        world.trust.revoked_packages.insert(hash(0));
        world
    };
    let unsigned = Package {
        signer: Signer::Unsigned,
        ..package(1, "1.0.0")
    };
    let rows: Vec<(&str, World, Change, Vec<Refusal>)> = vec![
        (
            "a revoked package",
            revoked_package(),
            apply(package(1, "1.0.0")),
            vec![Refusal::RevokedPackage],
        ),
        (
            "an unsigned package when the setting is off",
            World::new(),
            apply(unsigned.clone()),
            vec![Refusal::Unsigned {
                setting_allows: false,
            }],
        ),
        (
            "an unsigned package that the admin did not confirm",
            unsigned_allowed(),
            apply(unsigned.clone()),
            vec![Refusal::Unsigned {
                setting_allows: true,
            }],
        ),
        (
            "a confirmation of another hash",
            unsigned_allowed(),
            apply_with(
                unsigned.clone(),
                Approval {
                    unsigned_hash: Some(hash(9)),
                    ..Approval::default()
                },
            ),
            vec![Refusal::Unsigned {
                setting_allows: true,
            }],
        ),
        (
            "a key that the trust bundle does not list",
            World::new(),
            apply(Package {
                signer: Signer::Key(STRANGER),
                ..package(1, "1.0.0")
            }),
            vec![Refusal::Unsigned {
                setting_allows: false,
            }],
        ),
        (
            "an unsigned update of a signed add-on",
            World::with(installed()),
            apply(Package {
                signer: Signer::Unsigned,
                ..package(2, "1.1.0")
            }),
            vec![
                Refusal::Unsigned {
                    setting_allows: false,
                },
                Refusal::PublisherChanged {
                    was: "Acme".into(),
                    now: None,
                },
            ],
        ),
        (
            "a store newer than the package",
            World::with(Installed {
                store_data_version: 2,
                ..installed()
            }),
            apply(package(2, "1.1.0")),
            vec![Refusal::DataTooNew {
                store: 2,
                package: 1,
            }],
        ),
        (
            "running deployments with Refuse",
            busy(),
            Change::Disable {
                addon: id(),
                when_busy: WhenBusy::Refuse,
            },
            vec![Refusal::RunningDeployments { count: 1 }],
        ),
        (
            "an uninstall while a deployment runs",
            busy(),
            Change::Uninstall { addon: id() },
            vec![Refusal::InUse(Usage {
                running_deployments: 1,
                ..Usage::default()
            })],
        ),
        (
            "a roll back with no previous package",
            World::with(installed()),
            Change::RollBack {
                addon: id(),
                when_busy: WhenBusy::Wait,
            },
            vec![Refusal::NoPrevious],
        ),
        (
            "a roll back to a level the server no longer serves",
            old_level(),
            Change::RollBack {
                addon: id(),
                when_busy: WhenBusy::Wait,
            },
            vec![Refusal::IncompatibleLevel {
                needs: 0,
                serves: 1..=1,
            }],
        ),
        (
            "a roll back to a revoked package",
            revoked_previous(),
            Change::RollBack {
                addon: id(),
                when_busy: WhenBusy::Wait,
            },
            vec![Refusal::RevokedPackage],
        ),
        (
            "an enable of an enabled add-on",
            World::with(installed()),
            Change::Enable { addon: id() },
            vec![Refusal::AlreadyEnabled],
        ),
        (
            "a disable of a disabled add-on",
            World::with(Installed {
                enabled: false,
                ..installed()
            }),
            Change::Disable {
                addon: id(),
                when_busy: WhenBusy::Wait,
            },
            vec![Refusal::AlreadyDisabled],
        ),
        (
            "a purge of an installed add-on",
            World::with(installed()),
            Change::Purge { addon: id() },
            vec![Refusal::NotUninstalled],
        ),
        (
            "an enable of an uninstalled add-on",
            World::with(Installed {
                enabled: false,
                uninstalled: true,
                ..installed()
            }),
            Change::Enable { addon: id() },
            vec![Refusal::Uninstalled],
        ),
        (
            "a change to an add-on that was never installed",
            World::new(),
            Change::Uninstall { addon: id() },
            vec![Refusal::UnknownAddOn],
        ),
    ];
    for (name, world, change, want) in rows {
        assert_eq!(world.refusals(&change), want, "{name}");
    }
}

#[test]
fn an_enable_checks_the_trust_bundle_again() {
    let mut world = World::with(Installed {
        enabled: false,
        ..installed()
    });
    world.trust.revoked_keys.insert(ACME);
    assert_eq!(
        world.refusals(&Change::Enable { addon: id() }),
        [Refusal::RevokedKey { key: ACME }]
    );
    world.trust.publishers[0] = publisher("Acme", ACME, "acme.billing");
    world.trust.revoked_keys.clear();
    assert_eq!(
        world.refusals(&Change::Enable { addon: id() }),
        [Refusal::KeyOutOfScope {
            publisher: "Acme".into(),
            id: id(),
        }]
    );
}

#[test]
fn an_allowed_hash_installs_although_its_key_is_revoked() {
    let mut world = World::new();
    world.trust.revoked_keys.insert(ACME);
    world.trust.allowed_packages.insert(hash(1));
    assert!(world.plan(&apply(package(1, "1.0.0"))).is_ok());
    assert_eq!(
        world.refusals(&apply(package(2, "1.0.0"))),
        [Refusal::RevokedKey { key: ACME }]
    );
}

#[test]
fn a_confirmed_unsigned_package_installs_with_a_badge() {
    let mut world = World::new();
    world.allow_unsigned = true;
    let plan = world
        .plan(&apply_with(
            Package {
                signer: Signer::Key(STRANGER),
                ..package(1, "1.0.0")
            },
            Approval {
                unsigned_hash: Some(hash(1)),
                ..Approval::default()
            },
        ))
        .expect("a confirmed package installs");
    assert!(plan.effects.contains(&Effect::Unsigned));
    assert_eq!(
        plan.steps.last(),
        Some(&Step::Commit(Commit::Packages {
            active: hash(1),
            previous: None,
            publisher: None,
            enabled: true,
        }))
    );
}

#[test]
fn another_publisher_cannot_update_a_signed_add_on() {
    let mut world = World::with(installed());
    world
        .trust
        .publishers
        .push(publisher("Mallory", STRANGER, "acme.orders"));
    assert_eq!(
        world.refusals(&apply(Package {
            signer: Signer::Key(STRANGER),
            ..package(2, "1.1.0")
        })),
        [Refusal::PublisherChanged {
            was: "Acme".into(),
            now: Some("Mallory".into()),
        }]
    );
}

#[test]
fn a_refusal_lists_every_reason() {
    let mut world = World::with(installed());
    world.trust.revoked_keys.insert(ACME);
    let refusals = world.refusals(&apply(Package {
        api_level: 5,
        ..package(2, "0.5.0")
    }));
    assert_eq!(
        refusals,
        [
            Refusal::IncompatibleLevel {
                needs: 5,
                serves: 0..=1,
            },
            Refusal::RevokedKey { key: ACME },
            Refusal::Downgrade {
                from: version("1.0.0"),
                to: version("0.5.0"),
            },
        ]
    );
}

#[test]
fn package_hashes_read_and_write_hex() {
    let text = "00".repeat(31) + "ff";
    let parsed: PackageHash = text.parse().expect("64 hex digits");
    assert_eq!(parsed.0[31], 0xff);
    assert_eq!(parsed.to_string(), text);
    assert!("FF".repeat(32).parse::<PackageHash>().is_err());
    assert!("00".parse::<PackageHash>().is_err());
}
