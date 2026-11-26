//! Machine-readable contract validation: catalog integrity, parity between JSON contracts and
//! Rust types, and drift between documentation and implementation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use qrcardapio::domain::commercial::{BillingCycle, CommercialCatalog, Lifecycle, catalog};
use qrcardapio::domain::entitlements::{ActorRole, Capability, MemberRole, Operation};
use qrcardapio::domain::events::EventType;

const MODULES: &str = include_str!("../contracts/commercial/modules.json");
const BUNDLES: &str = include_str!("../contracts/commercial/bundles.json");
const ADDONS: &str = include_str!("../contracts/commercial/addons.json");
const CYCLES: &str = include_str!("../contracts/commercial/billing-cycles.json");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn json(path: &str) -> Value {
    let raw = std::fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("{path}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn catalog_prices_match_the_commercial_proposal() {
    let catalog = catalog();
    let expected = [
        ("menu", 2990, Lifecycle::Available),
        ("delivery", 6990, Lifecycle::InternalPreview),
        ("tables", 3990, Lifecycle::Planned),
        ("kitchen", 2490, Lifecycle::Planned),
        ("fleet", 2990, Lifecycle::Planned),
        ("loyalty", 1990, Lifecycle::Planned),
        ("analytics", 1990, Lifecycle::Planned),
        ("pos", 6990, Lifecycle::Planned),
        ("connect", 2490, Lifecycle::Planned),
    ];
    assert_eq!(catalog.modules.len(), expected.len());
    let mut total = 0;
    for (code, price, lifecycle) in expected {
        let module = catalog.module(code).unwrap();
        assert_eq!(module.current().monthly_price_centavos, price, "{code}");
        assert_eq!(module.lifecycle, lifecycle, "{code}");
        assert!(module.requires_modules.is_empty(), "{code}");
        total += price;
    }
    assert_eq!(total, 32_910);
    for (code, price, modules) in [
        ("start", 8490, 2),
        ("business", 16990, 7),
        ("complete", 21990, 9),
    ] {
        let bundle = catalog.bundle(code).unwrap();
        let version = bundle.version(bundle.current_version).unwrap();
        assert_eq!(version.monthly_price_centavos, price);
        assert_eq!(version.modules.len(), modules);
        assert_eq!(bundle.lifecycle, Lifecycle::Planned);
    }
    let domain = catalog.addon("custom_domain").unwrap();
    assert_eq!(
        domain
            .current_version
            .and_then(|v| domain.version(v))
            .unwrap()
            .monthly_price_centavos,
        1490
    );
    let cycles: Vec<_> = BillingCycle::ALL
        .iter()
        .map(|c| (c.months(), c.discount_bps()))
        .collect();
    assert_eq!(cycles, [(1, 0), (6, 500), (12, 1000)]);
    assert_eq!(catalog.payment_grace_days, 0);
}

fn load_with(edit: impl FnOnce(&mut Value, &mut Value, &mut Value, &mut Value)) -> Vec<String> {
    let mut m: Value = serde_json::from_str(MODULES).unwrap();
    let mut b: Value = serde_json::from_str(BUNDLES).unwrap();
    let mut a: Value = serde_json::from_str(ADDONS).unwrap();
    let mut c: Value = serde_json::from_str(CYCLES).unwrap();
    edit(&mut m, &mut b, &mut a, &mut c);
    match CommercialCatalog::load(
        &m.to_string(),
        &b.to_string(),
        &a.to_string(),
        &c.to_string(),
    ) {
        Ok(_) => Vec::new(),
        Err(problems) => problems,
    }
}

fn rejects(problems: &[String], needle: &str) {
    assert!(
        problems.iter().any(|p| p.contains(needle)),
        "expected a problem containing '{needle}', got {problems:?}"
    );
}

#[test]
fn invalid_catalogs_are_rejected() {
    assert!(load_with(|_, _, _, _| {}).is_empty());
    rejects(
        &load_with(|m, _, _, _| m["modules"][1]["code"] = "menu".into()),
        "duplicate offer code",
    );
    rejects(
        &load_with(|m, _, _, _| {
            m["modules"][0]["versions"][0]["capabilities"] = serde_json::json!(["menu.premium"])
        }),
        "unknown capability",
    );
    rejects(
        &load_with(|m, _, _, _| {
            m["modules"][0]["versions"][0]["capabilities"] =
                serde_json::json!(["delivery.order.create"])
        }),
        "belongs to delivery",
    );
    rejects(
        &load_with(|m, _, _, _| m["modules"][1]["requires_modules"] = serde_json::json!(["menu"])),
        "must not require",
    );
    rejects(
        &load_with(|m, _, _, _| {
            m["modules"][1]["requires_modules"] = serde_json::json!(["tables"]);
            m["modules"][2]["requires_modules"] = serde_json::json!(["delivery"]);
        }),
        "circular module dependency",
    );
    rejects(
        &load_with(|_, b, _, _| b["bundles"][0]["lifecycle"] = "available".into()),
        "contains internal_preview module delivery",
    );
    rejects(
        &load_with(|_, b, _, _| {
            b["bundles"][0]["versions"][0]["monthly_price_centavos"] = 9980.into()
        }),
        "below the sum",
    );
    rejects(
        &load_with(|_, b, _, _| {
            b["bundles"][0]["versions"][0]["modules"][1]["code"] = "drive".into()
        }),
        "unknown module drive",
    );
    rejects(
        &load_with(|m, _, _, _| {
            m["modules"][0]["versions"][0]["monthly_price_centavos"] = 0.into()
        }),
        "price must be positive",
    );
    rejects(
        &load_with(|m, _, _, _| m["modules"][0]["current_version"] = 2.into()),
        "current_version 2 does not exist",
    );
    rejects(
        &load_with(|m, _, _, _| m["modules"][0]["lifecycle"] = "planned".into()),
        "entry offer",
    );
    rejects(
        &load_with(|_, _, _, c| c["cycles"][1]["discount_bps"] = 10_000.into()),
        "invalid discount",
    );
    rejects(
        &load_with(|_, _, _, c| c["cycles"][2]["months"] = 11.into()),
        "must last 12 months",
    );
    rejects(
        &load_with(|_, _, a, _| a["addons"][0]["versions"] = serde_json::json!([])),
        "recurring add-on needs versions",
    );
    rejects(
        &load_with(|m, _, _, _| {
            let removed = m["modules"].as_array_mut().unwrap().pop().unwrap();
            assert_eq!(removed["code"], "connect");
        }),
        "capability connect.api.use is not granted",
    );
}

#[test]
fn capability_registry_matches_the_rust_enum() {
    let registry = json("contracts/entitlements/capabilities.json");
    let mut ids = BTreeSet::new();
    for entry in registry["capabilities"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        assert!(ids.insert(id.to_owned()), "duplicate capability {id}");
        let capability = Capability::parse(id).unwrap_or_else(|| panic!("{id} missing in Rust"));
        assert_eq!(entry["offer"], capability.offer(), "{id}");
        let lifecycle = catalog().capability_lifecycle(capability);
        let expected = catalog_contract_status(lifecycle);
        assert_eq!(entry["status"], expected, "{id}");
    }
    let rust: BTreeSet<String> = Capability::ALL
        .iter()
        .map(|c| c.as_str().to_owned())
        .collect();
    assert_eq!(ids, rust);
}

fn catalog_contract_status(lifecycle: Lifecycle) -> &'static str {
    match lifecycle {
        Lifecycle::Available => "implemented",
        Lifecycle::Development | Lifecycle::InternalPreview => lifecycle.as_str(),
        _ => "planned",
    }
}

#[test]
fn lifecycle_contract_matches_the_rust_rules() {
    let contract = json("contracts/commercial/module-lifecycle.json");
    let states = contract["states"].as_array().unwrap();
    assert_eq!(states.len(), Lifecycle::ALL.len());
    for row in states {
        let state = Lifecycle::parse(row["state"].as_str().unwrap()).unwrap();
        let sale = match state.sale() {
            qrcardapio::domain::commercial::SalePolicy::Never => "never",
            qrcardapio::domain::commercial::SalePolicy::ExplicitEnablement => "explicit_enablement",
            qrcardapio::domain::commercial::SalePolicy::Standard => "standard",
        };
        let grants = match state.grants() {
            qrcardapio::domain::commercial::GrantPolicy::Never => "never",
            qrcardapio::domain::commercial::GrantPolicy::ExplicitGrantOnly => "explicit_grant_only",
            qrcardapio::domain::commercial::GrantPolicy::Always => "always",
        };
        assert_eq!(row["sale"], sale, "{state:?}");
        assert_eq!(row["grants"], grants, "{state:?}");
        assert_eq!(row["renewals"], state.renewals(), "{state:?}");
    }
    let allowed: BTreeSet<(String, String)> = contract["transitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t[0].as_str().unwrap().to_owned(),
                t[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    for from in Lifecycle::ALL {
        for to in Lifecycle::ALL {
            let key = (from.as_str().to_owned(), to.as_str().to_owned());
            assert_eq!(
                from.can_transition_to(to),
                allowed.contains(&key),
                "{key:?}"
            );
        }
    }
    let ever: BTreeSet<String> = strings(&contract["sellable_ever"]).into_iter().collect();
    for state in Lifecycle::ALL {
        assert_eq!(state.ever_sellable(), ever.contains(state.as_str()));
    }
}

#[test]
fn authorization_rules_match_implemented_operations() {
    let rules = json("contracts/entitlements/authorization-rules.json");
    let operations = rules["operations"].as_array().unwrap();
    let roles: BTreeSet<String> = rules["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["role"].as_str().unwrap().to_owned())
        .collect();
    let mut seen = BTreeSet::new();
    let mut required: BTreeSet<String> = BTreeSet::new();
    for row in operations {
        let name = row["operation"].as_str().unwrap();
        assert!(seen.insert(name.to_owned()), "duplicate operation {name}");
        for role in strings(&row["roles"]) {
            assert!(roles.contains(&role), "{name}: unknown role {role}");
        }
        if let Some(cap) = row["capability"].as_str() {
            assert!(
                Capability::parse(cap).is_some(),
                "{name}: unknown capability {cap}"
            );
            required.insert(cap.to_owned());
            assert_ne!(
                row["after_expiry"], "allow",
                "{name}: gated operations cannot be free"
            );
        }
        assert!(
            ["deny", "allow", "complete_existing"].contains(&row["after_expiry"].as_str().unwrap())
        );
    }
    for capability in Capability::ALL {
        assert!(
            required.contains(capability.as_str()),
            "{capability} guards no operation"
        );
    }
    let implemented: Vec<&Value> = operations
        .iter()
        .filter(|row| row["status"] == "implemented")
        .collect();
    assert_eq!(implemented.len(), Operation::ALL.len());
    for operation in Operation::ALL {
        let row = implemented
            .iter()
            .find(|row| row["operation"] == operation.as_str())
            .unwrap_or_else(|| panic!("{} not in contract", operation.as_str()));
        let contract_roles: BTreeSet<String> = strings(&row["roles"]).into_iter().collect();
        let rust_roles: BTreeSet<String> = operation
            .roles()
            .iter()
            .map(|r| r.as_str().to_owned())
            .collect();
        assert_eq!(contract_roles, rust_roles, "{}", operation.as_str());
        assert_eq!(
            row["capability"].as_str(),
            operation.capability().map(Capability::as_str),
            "{}",
            operation.as_str()
        );
        assert_eq!(row["after_expiry"], operation.after_expiry().as_str());
        if operation.as_str().starts_with("delivery.") {
            assert_eq!(
                row["lifecycle"],
                catalog().module("delivery").unwrap().lifecycle.as_str(),
                "{} lifecycle",
                operation.as_str()
            );
        }
    }
    for role in ActorRole::ALL {
        let row = rules["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["role"] == role.as_str())
            .unwrap();
        assert_eq!(row["status"], "implemented");
    }
    let _ = MemberRole::ALL;
}

#[test]
fn event_registry_matches_the_outbox_producers() {
    let registry = json("contracts/events/registry.json");
    let envelope = strings(&registry["envelope"]["required"]);
    for field in [
        "event_id",
        "event_type",
        "event_version",
        "establishment_id",
        "aggregate_id",
        "correlation_id",
        "causation_id",
        "occurred_at",
        "payload",
    ] {
        assert!(
            envelope.iter().any(|f| f == field),
            "envelope lacks {field}"
        );
    }
    let mut types = BTreeSet::new();
    for event in registry["events"].as_array().unwrap() {
        let name = event["type"].as_str().unwrap();
        assert!(types.insert(name.to_owned()), "duplicate event {name}");
        assert!(
            event["version"].as_u64().is_some_and(|v| v >= 1),
            "{name} has no version"
        );
        match event["status"].as_str().unwrap() {
            "implemented" => {
                let schema_path = format!("contracts/events/{}", event["schema"].as_str().unwrap());
                let schema = json(&schema_path);
                assert_eq!(schema["type"], "object", "{name}");
                assert_eq!(schema["additionalProperties"], false, "{name}");
                for forbidden in ["email", "name", "phone", "address", "document", "token"] {
                    assert!(
                        schema["properties"].get(forbidden).is_none(),
                        "{name} exposes {forbidden}"
                    );
                }
                let rust = EventType::ALL
                    .iter()
                    .find(|t| t.as_str() == name)
                    .unwrap_or_else(|| panic!("{name} not produced in Rust"));
                assert_eq!(event["version"], rust.version());
                assert_eq!(event["aggregate"], rust.aggregate_type());
            }
            "planned" => assert!(
                event["payload"].is_object(),
                "{name} lacks a planned payload"
            ),
            other => panic!("{name}: unknown status {other}"),
        }
    }
    for rust in EventType::ALL {
        let row = registry["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == rust.as_str())
            .unwrap();
        assert_eq!(row["status"], "implemented");
    }
}

#[test]
fn module_contracts_reference_existing_contracts() {
    let registry = json("contracts/events/registry.json");
    let events: BTreeSet<String> = registry["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_owned())
        .collect();
    let rules = json("contracts/entitlements/authorization-rules.json");
    let operations: BTreeMap<String, Value> = rules["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["operation"].as_str().unwrap().to_owned(), o.clone()))
        .collect();
    let components: BTreeSet<String> = json("contracts/core/components.json")["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    let machines = json("contracts/commerce/state-machines.json");
    for module in &catalog().modules {
        let contract = json(&format!("contracts/modules/{}.json", module.code));
        assert_eq!(contract["module"], module.code.as_str());
        let expected_status = catalog_contract_status(module.lifecycle);
        assert_eq!(contract["status"], expected_status, "{}", module.code);
        assert_eq!(
            contract["standalone"], true,
            "{} must work without other modules",
            module.code
        );
        let caps: BTreeSet<String> = strings(&contract["capabilities"]).into_iter().collect();
        let catalog_caps: BTreeSet<String> = module
            .current()
            .capabilities
            .iter()
            .map(|c| c.as_str().to_owned())
            .collect();
        assert_eq!(caps, catalog_caps, "{}", module.code);
        for dep in strings(&contract["core_dependencies"]) {
            assert!(
                components.contains(&dep),
                "{}: unknown core component {dep}",
                module.code
            );
        }
        for other in strings(&contract["optional_integrations"]) {
            assert!(
                catalog().module(&other).is_some() && other != module.code,
                "{}: bad integration {other}",
                module.code
            );
        }
        for op in strings(&contract["operations"]) {
            let row = operations
                .get(&op)
                .unwrap_or_else(|| panic!("{}: unknown operation {op}", module.code));
            if let Some(cap) = row["capability"].as_str() {
                assert!(
                    caps.contains(cap),
                    "{}: operation {op} needs foreign capability {cap}",
                    module.code
                );
            }
        }
        for list in ["publishes", "consumes"] {
            for event in strings(&contract["events"][list]) {
                assert!(
                    events.contains(&event),
                    "{}: unknown event {event}",
                    module.code
                );
            }
        }
        for machine in strings(&contract["state_machines"]) {
            assert!(
                machines["machines"].get(&machine).is_some(),
                "{}: unknown machine {machine}",
                module.code
            );
        }
        for endpoint in contract["http"].as_array().unwrap() {
            let status = endpoint["status"].as_str().unwrap();
            if module.lifecycle != Lifecycle::Available {
                assert_ne!(
                    status, "available",
                    "{} advertises {} as available",
                    module.code, endpoint["path"]
                );
                assert_eq!(
                    status, expected_status,
                    "{} endpoint {} status mismatch",
                    module.code, endpoint["path"]
                );
            }
            assert!(
                !endpoint["path"].as_str().unwrap().contains("/:"),
                "Axum 0.8 paths use {{id}}"
            );
        }
    }
}

#[test]
fn state_machines_are_consistent() {
    let machines = json("contracts/commerce/state-machines.json");
    let events: BTreeSet<String> = json("contracts/events/registry.json")["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_owned())
        .collect();
    for (name, machine) in machines["machines"].as_object().unwrap() {
        let states: BTreeSet<String> = strings(&machine["states"]).into_iter().collect();
        assert!(
            states.contains(machine["initial"].as_str().unwrap()),
            "{name}"
        );
        let terminal: BTreeSet<String> = strings(&machine["terminal"]).into_iter().collect();
        assert!(terminal.is_subset(&states), "{name}");
        for t in machine["transitions"].as_array().unwrap() {
            let (from, to) = (t["from"].as_str().unwrap(), t["to"].as_str().unwrap());
            assert!(
                states.contains(from) && states.contains(to),
                "{name}: {from}->{to}"
            );
            assert!(
                !terminal.contains(from),
                "{name}: leaves terminal state {from}"
            );
            assert!(
                !strings(&t["actors"]).is_empty(),
                "{name}: {from}->{to} has no actor"
            );
            if let Some(event) = t["event"].as_str() {
                assert!(events.contains(event), "{name}: unknown event {event}");
            }
        }
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing {}", path.display()))
}

const MODULE_HEADINGS: [&str; 27] = [
    "Commercial objective",
    "Target audience",
    "Problems solved",
    "MVP scope",
    "Future scope",
    "Domain entities",
    "Responsibilities",
    "Invariants",
    "Business rules",
    "State machines",
    "Permissions",
    "Commercial capabilities",
    "Technical dependencies",
    "Published events",
    "Consumed events",
    "HTTP contracts",
    "Integration contracts",
    "Conceptual persistence model",
    "Security requirements",
    "Privacy requirements",
    "Performance requirements",
    "Behavior without other modules",
    "Behavior integrated with other modules",
    "Behavior after suspension or cancellation",
    "Acceptance criteria",
    "Test scenarios",
    "Commercial launch requirements",
];

#[test]
fn module_documentation_matches_module_contracts() {
    for module in &catalog().modules {
        let doc = read(&root().join(format!("docs/modules/{}.md", module.code.to_uppercase())));
        let headings: Vec<&str> = doc.lines().filter_map(|l| l.strip_prefix("## ")).collect();
        let expected: Vec<String> = MODULE_HEADINGS
            .iter()
            .enumerate()
            .map(|(i, h)| format!("{}. {h}", i + 1))
            .collect();
        assert_eq!(headings, expected, "{} headings", module.code);
        assert!(
            doc.starts_with(&format!("# {}", module.name)),
            "{} title",
            module.code
        );
        let contract = json(&format!("contracts/modules/{}.json", module.code));
        let mut identifiers = strings(&contract["capabilities"]);
        identifiers.extend(strings(&contract["operations"]));
        identifiers.extend(strings(&contract["events"]["publishes"]));
        identifiers.extend(strings(&contract["events"]["consumes"]));
        for id in identifiers {
            assert!(
                doc.contains(&format!("`{id}`")),
                "{} doc does not mention `{id}`",
                module.code
            );
        }
        if module.lifecycle == Lifecycle::Development {
            assert!(
                doc.contains("In development"),
                "{} doc must say it is in development",
                module.code
            );
        } else if module.lifecycle == Lifecycle::InternalPreview {
            assert!(
                doc.contains("`internal_preview`"),
                "{} doc must state the internal preview lifecycle",
                module.code
            );
        } else if module.lifecycle != Lifecycle::Available {
            assert!(
                doc.contains("Planned"),
                "{} doc must say it is planned",
                module.code
            );
        }
    }
}

#[test]
fn module_catalog_document_matches_prices() {
    let doc = read(&root().join("docs/business/MODULE_CATALOG.md"));
    for module in &catalog().modules {
        let price = module.current().monthly_price_centavos;
        let row = doc
            .lines()
            .find(|l| l.starts_with(&format!("| `{}` ", module.code)))
            .unwrap_or_else(|| panic!("MODULE_CATALOG.md lacks {}", module.code));
        assert!(row.contains(&format!("| {price} |")), "{row}");
        assert!(
            row.contains(&format!("`{}`", module.lifecycle.as_str())),
            "{row}"
        );
    }
    for bundle in &catalog().bundles {
        let price = bundle
            .version(bundle.current_version)
            .unwrap()
            .monthly_price_centavos;
        let row = doc
            .lines()
            .find(|l| l.starts_with(&format!("| `{}` ", bundle.code)))
            .unwrap_or_else(|| panic!("MODULE_CATALOG.md lacks bundle {}", bundle.code));
        assert!(row.contains(&format!("| {price} |")), "{row}");
    }
}
