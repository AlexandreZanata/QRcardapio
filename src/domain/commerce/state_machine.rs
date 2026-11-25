//! Typed state machines backed by the embedded Commerce Core transition contract.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::domain::events::EventType;

const CONTRACT: &str = include_str!("../../../contracts/commerce/state-machines.json");

#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} in Commerce Core: {value}")]
pub struct UnknownValue {
    kind: &'static str,
    value: String,
}

macro_rules! contract_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }
        }

        impl FromStr for $name {
            type Err = UnknownValue;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(UnknownValue { kind: stringify!($name), value: value.to_owned() }),
                }
            }
        }
    };
}

contract_enum!(Machine {
    Order => "order", OrderPayment => "order_payment", Preparation => "preparation",
    Delivery => "delivery", TableSession => "table_session", CashSession => "cash_session",
});
contract_enum!(Actor {
    Owner => "owner", Manager => "manager", Cashier => "cashier", Waiter => "waiter",
    KitchenOperator => "kitchen_operator", Courier => "courier", Consumer => "consumer",
    System => "system", Gateway => "gateway",
});
contract_enum!(OrderState {
    Placed => "placed", Accepted => "accepted", Rejected => "rejected", Preparing => "preparing",
    Ready => "ready", Completed => "completed", Cancelled => "cancelled",
});
contract_enum!(OrderPaymentState {
    Pending => "pending", Confirmed => "confirmed", Failed => "failed", Expired => "expired",
    Cancelled => "cancelled", PartiallyRefunded => "partially_refunded", Refunded => "refunded",
    ChargebackOpen => "chargeback_open", ChargebackLost => "chargeback_lost",
});
contract_enum!(PreparationState {
    Queued => "queued", InProgress => "in_progress", Ready => "ready", HandedOff => "handed_off",
    Cancelled => "cancelled",
});
contract_enum!(DeliveryState {
    Pending => "pending", Assigned => "assigned", PickedUp => "picked_up", Delivered => "delivered",
    Failed => "failed", Returned => "returned", Cancelled => "cancelled",
});
contract_enum!(TableSessionState {
    Open => "open", BillRequested => "bill_requested", Closed => "closed", Voided => "voided",
});
contract_enum!(CashSessionState { Open => "open", Counting => "counting", Closed => "closed" });
contract_enum!(MachineEvent {
    OrderAccepted => "OrderAccepted", OrderRejected => "OrderRejected",
    OrderPreparing => "OrderPreparing", OrderReady => "OrderReady",
    OrderCompleted => "OrderCompleted", OrderCancelled => "OrderCancelled",
    PaymentConfirmed => "PaymentConfirmed", PaymentRefunded => "PaymentRefunded",
    DeliveryAssigned => "DeliveryAssigned", DeliveryCompleted => "DeliveryCompleted",
    DeliveryFailed => "DeliveryFailed", TableBillClosed => "TableBillClosed",
    CashRegisterClosed => "CashRegisterClosed",
});

impl MachineEvent {
    /// Only the persisted order machine has outbox producers in Increment 1.
    pub const fn implemented(self) -> Option<EventType> {
        match self {
            Self::OrderAccepted => Some(EventType::OrderAccepted),
            Self::OrderRejected => Some(EventType::OrderRejected),
            Self::OrderPreparing => Some(EventType::OrderPreparing),
            Self::OrderReady => Some(EventType::OrderReady),
            Self::OrderCompleted => Some(EventType::OrderCompleted),
            Self::OrderCancelled => Some(EventType::OrderCancelled),
            Self::PaymentConfirmed => Some(EventType::PaymentConfirmed),
            Self::PaymentRefunded => Some(EventType::PaymentRefunded),
            Self::DeliveryAssigned
            | Self::DeliveryCompleted
            | Self::DeliveryFailed
            | Self::TableBillClosed
            | Self::CashRegisterClosed => None,
        }
    }
}

macro_rules! machine_states {
    ($($machine:ident => $state:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum State { $($machine($state)),+ }

        impl State {
            pub fn parse(machine: Machine, value: &str) -> Result<Self, UnknownValue> {
                match machine { $(Machine::$machine => Ok(Self::$machine(value.parse()?))),+ }
            }

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$machine(state) => state.as_str()),+ }
            }

            pub const fn machine(self) -> Machine {
                match self { $(Self::$machine(_) => Machine::$machine),+ }
            }
        }

        impl Machine {
            pub fn states(self) -> Vec<State> {
                match self {
                    $(Self::$machine => $state::ALL.iter().copied().map(State::$machine).collect()),+
                }
            }
        }

        $(impl From<$state> for State {
            fn from(state: $state) -> Self { Self::$machine(state) }
        })+
    };
}

machine_states!(
    Order => OrderState,
    OrderPayment => OrderPaymentState,
    Preparation => PreparationState,
    Delivery => DeliveryState,
    TableSession => TableSessionState,
    CashSession => CashSessionState,
);

#[derive(Debug)]
pub struct Transition {
    actors: BTreeSet<Actor>,
    pub event: Option<MachineEvent>,
}

#[derive(Debug)]
pub struct MachineDefinition {
    pub aggregate: String,
    pub initial: State,
    terminal: BTreeSet<State>,
    transitions: BTreeMap<(State, State), Transition>,
}

impl MachineDefinition {
    pub fn is_terminal(&self, state: State) -> bool {
        self.terminal.contains(&state)
    }
}

pub fn definition(machine: Machine) -> &'static MachineDefinition {
    &MACHINES[&machine]
}

pub fn transition(
    machine: Machine,
    from: State,
    to: State,
    actor: Actor,
) -> Option<&'static Transition> {
    definition(machine)
        .transitions
        .get(&(from, to))
        .filter(|rule| rule.actors.contains(&actor))
}

pub fn can_transition(machine: Machine, from: State, to: State, actor: Actor) -> bool {
    transition(machine, from, to, actor).is_some()
}

#[derive(Deserialize)]
struct RawContract {
    contract_version: u32,
    machines: BTreeMap<String, RawMachine>,
}

#[derive(Deserialize)]
struct RawMachine {
    aggregate: String,
    initial: String,
    terminal: Vec<String>,
    states: Vec<String>,
    transitions: Vec<RawTransition>,
}

#[derive(Deserialize)]
struct RawTransition {
    from: String,
    to: String,
    actors: Vec<String>,
    event: Option<String>,
}

static MACHINES: LazyLock<BTreeMap<Machine, MachineDefinition>> = LazyLock::new(|| {
    let contract: RawContract = serde_json::from_str(CONTRACT).expect("invalid state machine JSON");
    assert_eq!(contract.contract_version, 1);
    let definitions: BTreeMap<_, _> = contract
        .machines
        .into_iter()
        .map(|(name, raw)| {
            let machine = name.parse().expect("unknown state machine");
            (machine, load_machine(machine, raw))
        })
        .collect();
    assert_eq!(
        definitions.len(),
        Machine::ALL.len(),
        "missing state machine"
    );
    definitions
});

fn load_machine(machine: Machine, raw: RawMachine) -> MachineDefinition {
    let parse = |value: &str| State::parse(machine, value).expect("unknown state");
    let states: BTreeSet<_> = raw.states.iter().map(|name| parse(name)).collect();
    assert_eq!(
        states.len(),
        raw.states.len(),
        "duplicate state in {machine:?}"
    );
    assert_eq!(
        states,
        machine.states().into_iter().collect(),
        "state drift in {machine:?}"
    );
    let initial = parse(&raw.initial);
    let terminal: BTreeSet<_> = raw.terminal.iter().map(|name| parse(name)).collect();
    assert_eq!(
        terminal.len(),
        raw.terminal.len(),
        "duplicate terminal state in {machine:?}"
    );
    assert!(
        !terminal.contains(&initial),
        "terminal initial state in {machine:?}"
    );

    let mut transitions = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for rule in raw.transitions {
        let from = parse(&rule.from);
        let to = parse(&rule.to);
        sources.insert(from);
        let actors: BTreeSet<_> = rule
            .actors
            .iter()
            .map(|name| name.parse().expect("unknown actor"))
            .collect();
        assert!(!actors.is_empty(), "missing actors for {from:?} -> {to:?}");
        assert_eq!(
            actors.len(),
            rule.actors.len(),
            "duplicate actor for {from:?} -> {to:?}"
        );
        let event = rule
            .event
            .map(|name| name.parse().expect("unknown machine event"));
        assert!(
            transitions
                .insert((from, to), Transition { actors, event })
                .is_none(),
            "duplicate transition {from:?} -> {to:?}"
        );
    }
    assert_eq!(
        states
            .difference(&sources)
            .copied()
            .collect::<BTreeSet<_>>(),
        terminal,
        "terminal state drift in {machine:?}"
    );
    MachineDefinition {
        aggregate: raw.aggregate,
        initial,
        terminal,
        transitions,
    }
}
