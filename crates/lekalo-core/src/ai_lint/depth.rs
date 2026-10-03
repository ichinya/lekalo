//! Iterative SCC condensation followed by a longest-path DAG pass.
//! Depth counts edges between components; recursion is reported separately.
use super::wire::{Depth, State};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn measure(
    dimension: &str,
    target: &str,
    edges: &[(String, String)],
    roots: &[String],
) -> Depth {
    let unknown = || Depth {
        dimension: dimension.into(),
        target: target.into(),
        maximum: State::Unknown,
        recursive_components: 0,
        witness: Vec::new(),
    };
    if edges.len() > 250_000 {
        return unknown();
    }
    let names: Vec<String> = roots
        .iter()
        .cloned()
        .chain(edges.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if names.len() > 50_000
        || names.len().saturating_add(edges.len()).saturating_mul(12) > super::input::MAX_WORK
    {
        return unknown();
    }
    let index: BTreeMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut outgoing = vec![Vec::new(); names.len()];
    let mut incoming = outgoing.clone();
    for (a, b) in edges {
        let (a, b) = (index[a.as_str()], index[b.as_str()]);
        outgoing[a].push(b);
        incoming[b].push(a);
    }
    for a in outgoing.iter_mut().chain(incoming.iter_mut()) {
        a.sort_unstable();
        a.dedup();
    }
    let mut reachable = vec![false; names.len()];
    let mut stack: Vec<usize> = roots
        .iter()
        .filter_map(|s| index.get(s.as_str()).copied())
        .collect();
    while let Some(v) = stack.pop() {
        if reachable[v] {
            continue;
        }
        reachable[v] = true;
        stack.extend(&outgoing[v]);
    }
    let mut seen = vec![false; names.len()];
    let mut finish = Vec::new();
    for seed in 0..names.len() {
        if !reachable[seed] || seen[seed] {
            continue;
        }
        let mut dfs = vec![(seed, false)];
        while let Some((v, done)) = dfs.pop() {
            if done {
                finish.push(v);
                continue;
            }
            if seen[v] {
                continue;
            }
            seen[v] = true;
            dfs.push((v, true));
            dfs.extend(
                outgoing[v]
                    .iter()
                    .rev()
                    .filter(|&&n| reachable[n] && !seen[n])
                    .map(|&n| (n, false)),
            );
        }
    }
    let mut components = vec![usize::MAX; names.len()];
    let mut representatives = Vec::new();
    let mut sizes = Vec::new();
    for &seed in finish.iter().rev() {
        if components[seed] != usize::MAX {
            continue;
        }
        let component = representatives.len();
        let mut members = Vec::new();
        stack.push(seed);
        while let Some(v) = stack.pop() {
            if components[v] != usize::MAX {
                continue;
            }
            components[v] = component;
            members.push(v);
            stack.extend(incoming[v].iter().filter(|&&n| reachable[n]));
        }
        members.sort_unstable();
        representatives.push(members[0]);
        sizes.push(members.len());
    }
    let mut dag = vec![BTreeSet::new(); representatives.len()];
    let mut recursive = vec![false; representatives.len()];
    for (i, ns) in outgoing.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        let a = components[i];
        recursive[a] |= sizes[a] > 1;
        for &n in ns {
            let b = components[n];
            if a == b {
                recursive[a] = true;
            } else {
                dag[a].insert(b);
            }
        }
    }
    let mut degree = vec![0usize; dag.len()];
    for ns in &dag {
        for &n in ns {
            degree[n] += 1;
        }
    }
    let mut ready: VecDeque<usize> = degree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == 0)
        .map(|(i, _)| i)
        .collect();
    let mut distance = vec![0u64; dag.len()];
    let mut predecessor = vec![None; dag.len()];
    while let Some(v) = ready.pop_front() {
        for &n in &dag[v] {
            if distance[v] + 1 > distance[n] {
                distance[n] = distance[v] + 1;
                predecessor[n] = Some(v);
            }
            degree[n] -= 1;
            if degree[n] == 0 {
                ready.push_back(n);
            }
        }
    }
    let max = distance.iter().copied().max().unwrap_or(0);
    let mut witness = Vec::new();
    if let Some(mut v) = distance.iter().position(|d| *d == max) {
        loop {
            witness.push(names[representatives[v]].clone());
            if let Some(p) = predecessor[v] {
                v = p;
            } else {
                break;
            }
        }
        witness.reverse();
    }
    // A truncated witness cannot be represented as a complete measurement.
    let maximum = if witness.len() > 32 {
        witness.truncate(32);
        State::Unknown
    } else {
        State::Known(max)
    };
    Depth {
        dimension: dimension.into(),
        target: target.into(),
        maximum,
        recursive_components: recursive.iter().filter(|&&r| r).count() as u64,
        witness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edges(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(a, b)| ((*a).into(), (*b).into()))
            .collect()
    }
    #[test]
    fn recursion_is_separate_from_condensed_depth_and_unreachable_fanout() {
        let d = measure(
            "semantic-dependency",
            "model",
            &edges(&[
                ("a", "b"),
                ("b", "a"),
                ("b", "c"),
                ("c", "d"),
                ("unselected", "x"),
                ("x", "y"),
            ]),
            &["a".into()],
        );
        assert_eq!(d.maximum, State::Known(2));
        assert_eq!(d.recursive_components, 1);
        assert_eq!(d.witness.len(), 3);
    }
    #[test]
    fn witness_bound_does_not_claim_a_complete_maximum() {
        let rows = (0..33)
            .map(|i| (format!("n{i}"), format!("n{}", i + 1)))
            .collect::<Vec<_>>();
        let d = measure("native-call", "target", &rows, &["n0".into()]);
        assert_eq!(d.maximum, State::Unknown);
        assert!(d.witness.len() <= 32);
    }
}
