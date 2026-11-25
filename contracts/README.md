# Machine-readable contracts

Versioned, validated contracts shared by the code, the tests and the documentation. Change a contract and its implementation in the same pull request; `cargo test --test commercial_contracts` fails on drift.

| Directory | Contract | Source of truth for | Consumed by |
|---|---|---|---|
| `commercial/modules.json`, `bundles.json`, `addons.json`, `billing-cycles.json` | Commercial catalog | Offer codes, prices (centavos), versions, lifecycles, bundle composition, cycle discounts | Compiled into the binary (`domain::commercial::catalog()`) |
| `commercial/module-lifecycle.json` | Lifecycle behavior | Documentation of `domain::commercial::Lifecycle` | Parity test |
| `entitlements/capabilities.json` | Capability registry | Documentation of `domain::entitlements::Capability` | Parity test |
| `entitlements/authorization-rules.json` | Operations, roles, after-expiry behavior | Implemented rows mirror `Operation`/`MemberRole`; planned rows are design contracts | Parity test |
| `events/registry.json`, `events/schemas/*.json` | Domain events | Event names, versions, payload schemas | Parity test (`domain::events::EventType`) |
| `commerce/state-machines.json` | Commerce Core state machines (planned) | Order, payment, preparation, delivery, table session, cash session | Consistency test, `docs/architecture/COMMERCE_CORE.md` |
| `core/components.json` | QR Core components and free-tier limits | Allowed module dependencies | Module contract test |
| `modules/<code>.json` | Per-module contract | Capabilities, operations, events, entities, planned HTTP surface, release gates | Module contract test, `docs/modules/<CODE>.md` |
| `mcp/`, `auth/` | Existing MCP and Google sign-in contracts | — | `tests/mcp.rs`, Google tests |

## OpenAPI

No commercial JSON API exists yet: the dashboard uses server-rendered forms, and the only machine interface is the MCP connector (`mcp/tools.json`). OpenAPI documents (`openapi/commercial.yaml`, `commerce.yaml`, `storefront.yaml`) will be generated only for endpoints that are implemented, starting with Commerce Core (phase 3) and QR Connect (phase 11). Planned endpoints are listed with `"status": "planned"` in `modules/*.json` and must never be published as available APIs.
