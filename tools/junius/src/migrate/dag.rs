//! Build the migration dependency DAG and topologically sort it.
//!
//! Edges:
//! - **Host before all plugins**: the last host migration precedes every
//!   plugin's first migration (host migrations are themselves chained).
//! - **Within a plugin**: filename order → implicit chain (migration N → N+1).
//! - **Cross-plugin**: explicit `-- @requires <plugin>:<name>` edges.
//!
//! Manifest-level dependencies do NOT imply ordering — migrations declare their
//! own order, because cleanup migrations flow against the manifest dep arrow.

use std::collections::{HashMap, HashSet};

use petgraph::algo::{tarjan_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};

use crate::migrate::discover::HOST_PLUGIN;
use crate::migrate::header::RequireEdge;

/// A migration as seen by the ordering pass.
pub struct OrderInput<'a> {
    pub plugin: &'a str,
    pub name: &'a str,
    pub requires: &'a [RequireEdge],
}

/// Why ordering failed.
#[derive(Debug, PartialEq, Eq)]
pub enum DagError {
    /// A dependency cycle, reported as the `plugin:name` labels involved.
    Cycle(Vec<String>),
    /// An `@requires` directive pointed at a migration that doesn't exist.
    UnknownRequire { from: String, target: String },
}

/// Topologically sort `items`, returning indices into `items` in apply order.
/// `items` must be in discovery order (host first, then each plugin, each sorted
/// by filename).
pub fn order(items: &[OrderInput]) -> Result<Vec<usize>, DagError> {
    let mut graph = DiGraph::<usize, ()>::new();
    let mut nodes = Vec::with_capacity(items.len());
    let mut by_label: HashMap<(&str, &str), NodeIndex> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let idx = graph.add_node(i);
        nodes.push(idx);
        by_label.insert((item.plugin, item.name), idx);
    }

    // Implicit within-plugin chains + host/plugin bookkeeping.
    let mut prev_in_plugin: HashMap<&str, NodeIndex> = HashMap::new();
    let mut host_last: Option<NodeIndex> = None;
    let mut plugin_first: Vec<NodeIndex> = Vec::new();
    let mut seen_plugin: HashSet<&str> = HashSet::new();
    for (i, item) in items.iter().enumerate() {
        let idx = nodes[i];
        if let Some(prev) = prev_in_plugin.get(item.plugin) {
            graph.add_edge(*prev, idx, ());
        }
        prev_in_plugin.insert(item.plugin, idx);

        if item.plugin == HOST_PLUGIN {
            host_last = Some(idx);
        } else if seen_plugin.insert(item.plugin) {
            plugin_first.push(idx);
        }
    }

    // Host before all plugins.
    if let Some(last) = host_last {
        for &first in &plugin_first {
            graph.add_edge(last, first, ());
        }
    }

    // Explicit cross-plugin @requires edges (required → dependent).
    for (i, item) in items.iter().enumerate() {
        for req in item.requires {
            match by_label.get(&(req.plugin.as_str(), req.name.as_str())) {
                Some(&target) => {
                    graph.add_edge(target, nodes[i], ());
                }
                None => {
                    return Err(DagError::UnknownRequire {
                        from: format!("{}:{}", item.plugin, item.name),
                        target: format!("{}:{}", req.plugin, req.name),
                    });
                }
            }
        }
    }

    let Ok(sorted) = toposort(&graph, None) else {
        let cycle = tarjan_scc(&graph)
            .into_iter()
            .find(|scc| {
                scc.len() > 1 || (scc.len() == 1 && graph.find_edge(scc[0], scc[0]).is_some())
            })
            .map(|scc| {
                scc.into_iter()
                    .map(|n| {
                        let i = graph[n];
                        format!("{}:{}", items[i].plugin, items[i].name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Err(DagError::Cycle(cycle));
    };
    Ok(sorted.into_iter().map(|n| graph[n]).collect())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests panic on unexpected ordering failures"
)]
mod tests {
    use super::*;

    fn req(plugin: &str, name: &str) -> RequireEdge {
        RequireEdge {
            plugin: plugin.into(),
            name: name.into(),
        }
    }

    fn pos(order: &[usize], idx: usize) -> usize {
        order.iter().position(|&i| i == idx).expect("index present")
    }

    #[test]
    fn linear_host_chain() {
        let none: Vec<RequireEdge> = vec![];
        let items = vec![
            OrderInput {
                plugin: "platform",
                name: "0001_users",
                requires: &none,
            },
            OrderInput {
                plugin: "platform",
                name: "0002_sessions",
                requires: &none,
            },
        ];
        assert_eq!(order(&items).unwrap(), vec![0, 1]);
    }

    #[test]
    fn host_precedes_plugins_and_requires_orders_cross_plugin() {
        let none: Vec<RequireEdge> = vec![];
        let a_req = vec![req("b", "0001_b")];
        // discovery order: host, then plugin a, then plugin b.
        let items = vec![
            OrderInput {
                plugin: "platform",
                name: "0001_users",
                requires: &none,
            },
            OrderInput {
                plugin: "a",
                name: "0001_a",
                requires: &a_req,
            },
            OrderInput {
                plugin: "b",
                name: "0001_b",
                requires: &none,
            },
        ];
        let ord = order(&items).unwrap();
        assert_eq!(pos(&ord, 0), 0, "host migration first");
        assert!(pos(&ord, 2) < pos(&ord, 1), "b:0001_b before a:0001_a");
    }

    #[test]
    fn cycle_is_reported() {
        let a_req = vec![req("b", "0001_b")];
        let b_req = vec![req("a", "0001_a")];
        let items = vec![
            OrderInput {
                plugin: "a",
                name: "0001_a",
                requires: &a_req,
            },
            OrderInput {
                plugin: "b",
                name: "0001_b",
                requires: &b_req,
            },
        ];
        match order(&items) {
            Err(DagError::Cycle(cycle)) => {
                assert!(cycle.contains(&"a:0001_a".to_string()));
                assert!(cycle.contains(&"b:0001_b".to_string()));
            }
            other => panic!("expected a cycle, got {other:?}"),
        }
    }

    #[test]
    fn unknown_require_is_reported() {
        let bad = vec![req("ghost", "0001_x")];
        let items = vec![OrderInput {
            plugin: "a",
            name: "0001_a",
            requires: &bad,
        }];
        assert_eq!(
            order(&items),
            Err(DagError::UnknownRequire {
                from: "a:0001_a".into(),
                target: "ghost:0001_x".into()
            })
        );
    }
}
