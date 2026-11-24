use std::collections::{BTreeMap, BTreeSet};

use super::{
    AddonPricing, BillingCycle, CapabilityVersion, CommercialCatalog, ENTRY_OFFER, SalePolicy,
};
use crate::domain::entitlements::Capability;

const MAX_GRACE_DAYS: i64 = 30;

/// Returns every structural or commercial inconsistency in the catalog contracts.
pub fn validate(catalog: &CommercialCatalog) -> Vec<String> {
    let mut problems = Vec::new();
    if catalog.currency != "BRL" {
        problems.push(format!("unsupported currency {}", catalog.currency));
    }
    check_codes(catalog, &mut problems);
    check_modules(catalog, &mut problems);
    check_bundles(catalog, &mut problems);
    check_addons(catalog, &mut problems);
    check_cycles(catalog, &mut problems);
    check_capability_ownership(catalog, &mut problems);
    match catalog.module(ENTRY_OFFER) {
        Some(module) if module.lifecycle.sale() == SalePolicy::Standard => {}
        _ => problems.push(format!(
            "entry offer '{ENTRY_OFFER}' must exist and be available"
        )),
    }
    problems
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 40
        && code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn check_codes(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    let codes = catalog
        .modules
        .iter()
        .map(|m| &m.code)
        .chain(catalog.bundles.iter().map(|b| &b.code))
        .chain(catalog.addons.iter().map(|a| &a.code));
    for code in codes {
        if !valid_code(code) {
            problems.push(format!("invalid offer code '{code}'"));
        }
        if !seen.insert(code.as_str()) {
            problems.push(format!("duplicate offer code '{code}'"));
        }
    }
}

fn check_versions<T>(
    offer: &str,
    versions: &[T],
    current: Option<u32>,
    version_of: impl Fn(&T) -> u32,
    problems: &mut Vec<String>,
) {
    for (index, item) in versions.iter().enumerate() {
        if usize::try_from(version_of(item)).ok() != Some(index + 1) {
            problems.push(format!("{offer}: versions must be numbered 1..n in order"));
            break;
        }
    }
    if let Some(current) = current
        && !versions.iter().any(|v| version_of(v) == current)
    {
        problems.push(format!("{offer}: current_version {current} does not exist"));
    }
}

fn check_capability_version(offer: &str, version: &CapabilityVersion, problems: &mut Vec<String>) {
    if version.monthly_price_centavos <= 0 {
        problems.push(format!(
            "{offer} v{}: price must be positive",
            version.version
        ));
    }
    if version.capabilities.is_empty() {
        problems.push(format!("{offer} v{}: no capabilities", version.version));
    }
    let unique: BTreeSet<_> = version.capabilities.iter().collect();
    if unique.len() != version.capabilities.len() {
        problems.push(format!(
            "{offer} v{}: duplicated capability",
            version.version
        ));
    }
    for capability in &version.capabilities {
        if capability.offer() != offer {
            problems.push(format!(
                "{offer} v{}: capability {capability} belongs to {}",
                version.version,
                capability.offer()
            ));
        }
    }
}

fn check_modules(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for module in &catalog.modules {
        check_versions(
            &module.code,
            &module.versions,
            Some(module.current_version),
            |v| v.version,
            problems,
        );
        for version in &module.versions {
            check_capability_version(&module.code, version, problems);
        }
        for required in &module.requires_modules {
            if catalog.module(required).is_none() {
                problems.push(format!(
                    "{}: requires unknown module {required}",
                    module.code
                ));
            }
            problems.push(format!(
                "{}: commercial modules must not require other modules ({required}); depend on QR Core instead",
                module.code
            ));
        }
        graph.insert(
            &module.code,
            module.requires_modules.iter().map(String::as_str).collect(),
        );
    }
    if let Some(cycle) = find_cycle(&graph) {
        problems.push(format!(
            "circular module dependency: {}",
            cycle.join(" -> ")
        ));
    }
}

fn find_cycle<'a>(graph: &BTreeMap<&'a str, Vec<&'a str>>) -> Option<Vec<&'a str>> {
    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        stack: &mut Vec<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Option<Vec<&'a str>> {
        if let Some(pos) = stack.iter().position(|n| *n == node) {
            let mut cycle = stack[pos..].to_vec();
            cycle.push(node);
            return Some(cycle);
        }
        if !done.insert(node) {
            return None;
        }
        stack.push(node);
        for next in graph.get(node).into_iter().flatten() {
            if let Some(cycle) = visit(next, graph, stack, done) {
                return Some(cycle);
            }
        }
        stack.pop();
        None
    }
    let mut done = BTreeSet::new();
    for node in graph.keys() {
        if let Some(cycle) = visit(node, graph, &mut Vec::new(), &mut done) {
            return Some(cycle);
        }
    }
    None
}

fn check_bundles(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    for bundle in &catalog.bundles {
        check_versions(
            &bundle.code,
            &bundle.versions,
            Some(bundle.current_version),
            |v| v.version,
            problems,
        );
        for version in &bundle.versions {
            let label = format!("{} v{}", bundle.code, version.version);
            if version.modules.len() < 2 {
                problems.push(format!("{label}: a bundle needs at least two modules"));
            }
            let mut members = BTreeSet::new();
            let mut parts_total = 0;
            for pinned in &version.modules {
                if !members.insert(pinned.code.as_str()) {
                    problems.push(format!("{label}: module {} listed twice", pinned.code));
                }
                let Some(module) = catalog.module(&pinned.code) else {
                    problems.push(format!("{label}: unknown module {}", pinned.code));
                    continue;
                };
                match module.version(pinned.version) {
                    Some(v) => parts_total += v.monthly_price_centavos,
                    None => problems.push(format!(
                        "{label}: module {} has no version {}",
                        pinned.code, pinned.version
                    )),
                }
                if bundle.lifecycle.sale().rank() > module.lifecycle.sale().rank() {
                    problems.push(format!(
                        "{label}: bundle is {} but contains {} module {}",
                        bundle.lifecycle.as_str(),
                        module.lifecycle.as_str(),
                        module.code
                    ));
                }
            }
            if version.monthly_price_centavos <= 0 || version.monthly_price_centavos >= parts_total
            {
                problems.push(format!(
                    "{label}: price {} must be positive and below the sum of its modules ({parts_total})",
                    version.monthly_price_centavos
                ));
            }
        }
    }
}

fn check_addons(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    for addon in &catalog.addons {
        match addon.pricing {
            AddonPricing::Recurring => {
                if addon.current_version.is_none() || addon.versions.is_empty() {
                    problems.push(format!("{}: recurring add-on needs versions", addon.code));
                }
                check_versions(
                    &addon.code,
                    &addon.versions,
                    addon.current_version,
                    |v| v.version,
                    problems,
                );
                for version in &addon.versions {
                    check_capability_version(&addon.code, version, problems);
                }
            }
            AddonPricing::OneTimeQuote | AddonPricing::CustomQuote | AddonPricing::UsageBased => {
                if addon.current_version.is_some() || !addon.versions.is_empty() {
                    problems.push(format!(
                        "{}: quoted or usage-based add-ons cannot have recurring versions",
                        addon.code
                    ));
                }
            }
        }
    }
}

fn check_cycles(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    if catalog.rounding != "half_up_per_line" {
        problems.push(format!("unsupported rounding rule {}", catalog.rounding));
    }
    if !(0..=MAX_GRACE_DAYS).contains(&catalog.payment_grace_days) {
        problems.push("payment_grace_days must be between 0 and 30".into());
    }
    if catalog.cycles.len() != BillingCycle::ALL.len() {
        problems.push("billing cycles must match BillingCycle exactly".into());
    }
    for cycle in BillingCycle::ALL {
        match catalog.cycles.iter().find(|c| c.code == cycle.key()) {
            None => problems.push(format!("missing billing cycle {}", cycle.key())),
            Some(def) => {
                if def.months != cycle.months() {
                    problems.push(format!(
                        "cycle {} must last {} months",
                        def.code,
                        cycle.months()
                    ));
                }
                if !(0..10_000).contains(&def.discount_bps) {
                    problems.push(format!("cycle {} has an invalid discount", def.code));
                }
                if def.gateway_cycle.is_empty() {
                    problems.push(format!("cycle {} needs a gateway cycle", def.code));
                }
            }
        }
    }
}

fn check_capability_ownership(catalog: &CommercialCatalog, problems: &mut Vec<String>) {
    let mut granted: BTreeSet<Capability> = BTreeSet::new();
    let versions = catalog
        .modules
        .iter()
        .flat_map(|m| m.versions.iter())
        .chain(catalog.addons.iter().flat_map(|a| a.versions.iter()));
    for version in versions {
        granted.extend(version.capabilities.iter().copied());
    }
    for capability in Capability::ALL {
        if !granted.contains(&capability) {
            problems.push(format!(
                "capability {capability} is not granted by any offer"
            ));
        }
    }
}
