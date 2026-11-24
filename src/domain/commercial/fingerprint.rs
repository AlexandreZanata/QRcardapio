use super::{CommercialCatalog, Lifecycle, OfferKind};

/// Canonical description of one offer version. Its hash is recorded once the offer is
/// sellable so a later release cannot change the terms of an existing version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFingerprint {
    pub kind: OfferKind,
    pub code: String,
    pub version: u32,
    pub canonical: String,
}

/// Registry view of one offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferFingerprint {
    pub kind: OfferKind,
    pub code: String,
    pub lifecycle: Lifecycle,
    pub current_version: Option<u32>,
    pub versions: Vec<VersionFingerprint>,
}

fn canonical(
    kind: OfferKind,
    code: &str,
    version: u32,
    currency: &str,
    monthly: i64,
    mut parts: Vec<String>,
) -> String {
    parts.sort();
    format!(
        "{}|{code}|v{version}|{currency}|{monthly}|{}",
        kind.as_str(),
        parts.join(",")
    )
}

impl CommercialCatalog {
    pub fn fingerprints(&self) -> Vec<OfferFingerprint> {
        let mut out = Vec::new();
        for module in &self.modules {
            out.push(OfferFingerprint {
                kind: OfferKind::Module,
                code: module.code.clone(),
                lifecycle: module.lifecycle,
                current_version: Some(module.current_version),
                versions: module
                    .versions
                    .iter()
                    .map(|v| VersionFingerprint {
                        kind: OfferKind::Module,
                        code: module.code.clone(),
                        version: v.version,
                        canonical: canonical(
                            OfferKind::Module,
                            &module.code,
                            v.version,
                            &self.currency,
                            v.monthly_price_centavos,
                            v.capabilities
                                .iter()
                                .map(|c| c.as_str().to_owned())
                                .collect(),
                        ),
                    })
                    .collect(),
            });
        }
        for bundle in &self.bundles {
            out.push(OfferFingerprint {
                kind: OfferKind::Bundle,
                code: bundle.code.clone(),
                lifecycle: bundle.lifecycle,
                current_version: Some(bundle.current_version),
                versions: bundle
                    .versions
                    .iter()
                    .map(|v| VersionFingerprint {
                        kind: OfferKind::Bundle,
                        code: bundle.code.clone(),
                        version: v.version,
                        canonical: canonical(
                            OfferKind::Bundle,
                            &bundle.code,
                            v.version,
                            &self.currency,
                            v.monthly_price_centavos,
                            v.modules
                                .iter()
                                .map(|m| format!("{}@{}", m.code, m.version))
                                .collect(),
                        ),
                    })
                    .collect(),
            });
        }
        for addon in &self.addons {
            out.push(OfferFingerprint {
                kind: OfferKind::Addon,
                code: addon.code.clone(),
                lifecycle: addon.lifecycle,
                current_version: addon.current_version,
                versions: addon
                    .versions
                    .iter()
                    .map(|v| VersionFingerprint {
                        kind: OfferKind::Addon,
                        code: addon.code.clone(),
                        version: v.version,
                        canonical: canonical(
                            OfferKind::Addon,
                            &addon.code,
                            v.version,
                            &self.currency,
                            v.monthly_price_centavos,
                            v.capabilities
                                .iter()
                                .map(|c| c.as_str().to_owned())
                                .collect(),
                        ),
                    })
                    .collect(),
            });
        }
        out
    }
}
