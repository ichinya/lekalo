//! The deterministic local transition graph (issue #63).
//!
//! Edges are built from every declared from-state to its target state
//! with the owning transition; state and transition identifiers sort
//! by unsigned UTF-8 bytes. The checks are local to this attachment's
//! declared state machine — distinct from the #13 generic dependency
//! graph, which may consume typed edges but never infers state
//! semantics. Bounded iterative reachability and Tarjan-style SCC
//! detection only: no recursive traversal, no unbounded closure.

use std::collections::BTreeMap;

use super::diagnostic::{self, GRAPH_INVALID};
use crate::diagnostics::DiagnosticSet;

use super::state::CyclePolicy;
use super::InvariantTransitionAttachment;

/// One deterministic graph check outcome over one state space.
pub(crate) fn check(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    for space in attachment.state_spaces() {
        let space_id = space.state_space_id().as_str();
        let transitions: Vec<&super::transition::Transition> = attachment
            .transitions()
            .iter()
            .filter(|transition| transition.state_space_id().as_str() == space_id)
            .collect();
        check_edges(&transitions)?;
        check_cycles(space, &transitions)?;
        check_reachability(space, &transitions)?;
    }
    Ok(())
}

/// Duplicate transition detection (same from-set, target, and command)
/// and edge endpoint validation.
fn check_edges(transitions: &[&super::transition::Transition]) -> Result<(), DiagnosticSet> {
    let mut signatures: Vec<String> = Vec::new();
    for transition in transitions {
        let signature = format!(
            "{}->{}:command={}",
            transition
                .from_states()
                .iter()
                .map(|state| state.as_str())
                .collect::<Vec<_>>()
                .join(","),
            transition.to_state().as_str(),
            transition.command().as_str()
        );
        if signatures.contains(&signature) {
            return Err(diagnostic::rule_invalid(
                GRAPH_INVALID,
                "duplicate-transition",
                Some(transition.transition_id().as_str()),
            ));
        }
        signatures.push(signature);
    }
    Ok(())
}

/// Bounded iterative SCC detection with the explicit cycle policy.
/// Allowed cycles are recorded as facts by the caller; forbidden
/// self or mutual cycles are invalid.
fn check_cycles(
    space: &super::state::StateSpace,
    transitions: &[&super::transition::Transition],
) -> Result<(), DiagnosticSet> {
    if space.cycle_policy() != CyclePolicy::Forbid {
        return Ok(());
    }
    // Iterative Tarjan SCC over the state graph.
    let adjacency = adjacency_map(transitions);
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut lowlink: BTreeMap<&str, usize> = BTreeMap::new();
    let mut on_stack: Vec<&str> = Vec::new();
    let mut counter = 0usize;
    for state in space.states() {
        let start = state.state_id().as_str();
        if index.contains_key(start) {
            continue;
        }
        // Explicit work stack: (node, next-neighbor cursor).
        let mut work: Vec<(&str, usize)> = vec![(start, 0)];
        while let Some((node, cursor_init)) = work.pop() {
            let mut cursor = cursor_init;
            let neighbors = adjacency.get(node).cloned().unwrap_or_default();
            if cursor == 0 {
                index.insert(node, counter);
                lowlink.insert(node, counter);
                counter += 1;
                on_stack.push(node);
            }
            let mut advanced = false;
            while cursor < neighbors.len() {
                let neighbor = neighbors[cursor];
                if !index.contains_key(neighbor) {
                    work.push((node, cursor + 1));
                    work.push((neighbor, 0));
                    advanced = true;
                    break;
                }
                if on_stack.contains(&neighbor) {
                    let neighbor_index = index[neighbor];
                    let current = lowlink[node];
                    if neighbor_index < current {
                        lowlink.insert(node, neighbor_index);
                    }
                }
                cursor += 1;
            }
            if advanced {
                continue;
            }
            // All neighbors processed: fold into the parent (if any).
            if let Some(low) = lowlink.get(node).copied() {
                if let Some(parent) = work.last().map(|(node, _)| *node) {
                    let parent_low = lowlink.get(parent).copied().unwrap_or(usize::MAX);
                    if low < parent_low {
                        lowlink.insert(parent, low);
                    }
                }
            }
            if lowlink.get(node) == index.get(node) {
                let mut size = 0usize;
                let mut has_edge = false;
                while let Some(top) = on_stack.pop() {
                    size += 1;
                    if adjacency.get(top).map(|neighbors| neighbors.contains(&top)) == Some(true) {
                        has_edge = true;
                    }
                    if top == node {
                        break;
                    }
                }
                // A self-edge or an SCC with more than one member is a
                // cycle; both are forbidden here.
                if size > 1 || has_edge {
                    return Err(diagnostic::rule_invalid(
                        GRAPH_INVALID,
                        "forbidden-cycle",
                        Some(node),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The sorted adjacency map of the declared state graph.
fn adjacency_map<'a>(
    transitions: &[&'a super::transition::Transition],
) -> BTreeMap<&'a str, Vec<&'a str>> {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for transition in transitions {
        for from in transition.from_states() {
            adjacency
                .entry(from.as_str())
                .or_default()
                .push(transition.to_state().as_str());
        }
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    adjacency
}

/// Bounded iterative reachability: every declared state is reachable
/// from an initial state, every transition is reachable, and the dead
/// nonterminal policy is honored. Self-transitions count as outgoing
/// edges, so the check never fires on a legal self-loop when the
/// policy allows it.
fn check_reachability(
    space: &super::state::StateSpace,
    transitions: &[&super::transition::Transition],
) -> Result<(), DiagnosticSet> {
    let adjacency = adjacency_map(transitions);
    let mut reachable: Vec<&str> = Vec::new();
    let mut queue: Vec<&str> = Vec::new();
    for state in space.states() {
        if state.initial() {
            queue.push(state.state_id().as_str());
        }
    }
    while let Some(state) = queue.pop() {
        if reachable.contains(&state) {
            continue;
        }
        reachable.push(state);
        if let Some(neighbors) = adjacency.get(state) {
            for neighbor in neighbors {
                if !reachable.contains(neighbor) {
                    queue.push(neighbor);
                }
            }
        }
    }
    for state in space.states() {
        if !reachable.contains(&state.state_id().as_str()) {
            return Err(diagnostic::rule_invalid(
                GRAPH_INVALID,
                "unreachable-state",
                Some(state.state_id().as_str()),
            ));
        }
    }
    for transition in transitions {
        let reachable_from = transition
            .from_states()
            .iter()
            .all(|from| reachable.contains(&from.as_str()));
        if !reachable_from {
            return Err(diagnostic::rule_invalid(
                GRAPH_INVALID,
                "unreachable-transition",
                Some(transition.transition_id().as_str()),
            ));
        }
    }
    if space.dead_policy() == super::state::DeadPolicy::Forbid {
        for state in space.states() {
            if state.terminal() {
                continue;
            }
            let outgoing = adjacency
                .get(state.state_id().as_str())
                .map(|neighbors| !neighbors.is_empty())
                .unwrap_or(false);
            if !outgoing {
                return Err(diagnostic::rule_invalid(
                    GRAPH_INVALID,
                    "dead-nonterminal-state",
                    Some(state.state_id().as_str()),
                ));
            }
        }
    }
    Ok(())
}
