use std::collections::BTreeSet;

use qrcardapio::domain::commerce::state_machine::{
    Actor, Machine, State, can_transition, definition, transition,
};
use serde_json::Value;

fn strings(value: &Value) -> BTreeSet<&str> {
    let values = value.as_array().unwrap();
    let set: BTreeSet<_> = values.iter().map(|value| value.as_str().unwrap()).collect();
    assert_eq!(values.len(), set.len(), "duplicate contract entry");
    set
}

#[test]
fn six_machines_match_every_state_actor_pair_and_event() {
    let contract: Value =
        serde_json::from_str(include_str!("../contracts/commerce/state-machines.json")).unwrap();
    let machines = contract["machines"].as_object().unwrap();
    let machine_names: BTreeSet<_> = Machine::ALL
        .iter()
        .map(|machine| machine.as_str())
        .collect();
    assert_eq!(
        machines.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        machine_names
    );
    let registry: Value =
        serde_json::from_str(include_str!("../contracts/events/registry.json")).unwrap();
    let registered = registry["events"].as_array().unwrap();
    let mut contract_actors = BTreeSet::new();
    let all_states: Vec<State> = Machine::ALL
        .iter()
        .flat_map(|machine| machine.states())
        .collect();

    for &machine in Machine::ALL {
        let raw = &machines[machine.as_str()];
        let actual = definition(machine);
        let states = machine.states();
        assert_eq!(raw["aggregate"], actual.aggregate);
        assert_eq!(raw["initial"], actual.initial.as_str());
        assert_eq!(
            strings(&raw["states"]),
            states.iter().map(|state| state.as_str()).collect()
        );
        assert_eq!(
            strings(&raw["terminal"]),
            states
                .iter()
                .filter(|&&state| actual.is_terminal(state))
                .map(|state| state.as_str())
                .collect()
        );

        let rules = raw["transitions"].as_array().unwrap();
        let mut pairs = BTreeSet::new();
        for rule in rules {
            let pair = (rule["from"].as_str().unwrap(), rule["to"].as_str().unwrap());
            assert!(pairs.insert(pair), "duplicate rule {machine:?} {pair:?}");
            let actors = strings(&rule["actors"]);
            assert!(!actors.is_empty());
            contract_actors.extend(actors);
            assert!(rule.get("event").is_some());
            if let Some(event) = rule["event"].as_str() {
                let entry = registered
                    .iter()
                    .find(|entry| entry["type"] == event)
                    .unwrap();
                assert_eq!(entry["aggregate"], actual.aggregate);
            }
        }

        for &from in &all_states {
            for &to in &all_states {
                for &actor in Actor::ALL {
                    let expected = rules.iter().find(|rule| {
                        from.machine() == machine
                            && to.machine() == machine
                            && rule["from"] == from.as_str()
                            && rule["to"] == to.as_str()
                            && strings(&rule["actors"]).contains(actor.as_str())
                    });
                    let actual = transition(machine, from, to, actor);
                    let context = (machine, from, to, actor);
                    assert_eq!(
                        can_transition(machine, from, to, actor),
                        expected.is_some(),
                        "{context:?}"
                    );
                    assert_eq!(
                        actual.map(|rule| rule.event.map(|event| event.as_str())),
                        expected.map(|rule| rule["event"].as_str()),
                        "{context:?}"
                    );
                    let emitted = expected
                        .and_then(|rule| rule["event"].as_str())
                        .filter(|name| {
                            registered.iter().any(|entry| {
                                entry["type"] == *name && entry["status"] == "implemented"
                            })
                        });
                    assert_eq!(
                        actual
                            .and_then(|rule| rule.event)
                            .and_then(|event| event.implemented())
                            .map(|event| event.as_str()),
                        emitted,
                        "{context:?}"
                    );
                }
            }
        }
    }
    assert_eq!(
        contract_actors,
        Actor::ALL.iter().map(|actor| actor.as_str()).collect()
    );
}
