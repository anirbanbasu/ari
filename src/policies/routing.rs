// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Routing Policies
//!
//! Pluggable routing algorithms for RINA.

use std::collections::HashMap;

/// Trait for routing policies
pub trait RoutingPolicy: Send + Sync {
    /// Computes the next hop for a destination
    fn compute_next_hop(&self, src: u64, dst: u64, topology: &NetworkTopology) -> Option<u64>;
    
    /// Updates routing information based on topology changes
    fn update(&mut self, topology: &NetworkTopology);
    
    /// Returns the policy name
    fn name(&self) -> &str;
}

/// Network topology information
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// Adjacency list: node -> (neighbor, cost)
    pub adjacency: HashMap<u64, Vec<(u64, u32)>>,
}

impl NetworkTopology {
    pub fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
        }
    }

    pub fn add_link(&mut self, from: u64, to: u64, cost: u32) {
        self.adjacency
            .entry(from)
            .or_insert_with(Vec::new)
            .push((to, cost));
    }

    pub fn get_neighbors(&self, node: u64) -> Vec<(u64, u32)> {
        self.adjacency.get(&node).cloned().unwrap_or_default()
    }
}

impl Default for NetworkTopology {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple shortest-path routing using Dijkstra's algorithm
#[derive(Debug)]
pub struct ShortestPathRouting {
    /// Computed routing table: (src, dst) -> next_hop
    routing_table: HashMap<(u64, u64), u64>,
}

impl ShortestPathRouting {
    pub fn new() -> Self {
        Self {
            routing_table: HashMap::new(),
        }
    }

    /// Computes shortest paths using Dijkstra's algorithm
    fn compute_shortest_paths(&mut self, source: u64, topology: &NetworkTopology) {
        let mut distances: HashMap<u64, u32> = HashMap::new();
        let mut previous: HashMap<u64, u64> = HashMap::new();
        let mut unvisited: Vec<u64> = topology.adjacency.keys().copied().collect();

        distances.insert(source, 0);

        while !unvisited.is_empty() {
            // Find node with minimum distance
            let current = unvisited
                .iter()
                .min_by_key(|&&node| distances.get(&node).unwrap_or(&u32::MAX))
                .copied()
                .unwrap();

            unvisited.retain(|&x| x != current);

            let current_distance = *distances.get(&current).unwrap_or(&u32::MAX);
            if current_distance == u32::MAX {
                break;
            }

            // Update distances to neighbors
            for (neighbor, cost) in topology.get_neighbors(current) {
                let new_distance = current_distance.saturating_add(cost);
                let neighbor_distance = *distances.get(&neighbor).unwrap_or(&u32::MAX);

                if new_distance < neighbor_distance {
                    distances.insert(neighbor, new_distance);
                    previous.insert(neighbor, current);
                }
            }
        }

        // Build routing table from previous pointers
        for (&dest, _) in &distances {
            if dest == source {
                continue;
            }

            // Trace back to find first hop
            let mut node = dest;
            while let Some(&prev) = previous.get(&node) {
                if prev == source {
                    self.routing_table.insert((source, dest), node);
                    break;
                }
                node = prev;
            }
        }
    }
}

impl Default for ShortestPathRouting {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingPolicy for ShortestPathRouting {
    fn compute_next_hop(&self, src: u64, dst: u64, _topology: &NetworkTopology) -> Option<u64> {
        self.routing_table.get(&(src, dst)).copied()
    }

    fn update(&mut self, topology: &NetworkTopology) {
        self.routing_table.clear();
        
        // Compute shortest paths from all nodes
        for &source in topology.adjacency.keys() {
            self.compute_shortest_paths(source, topology);
        }
    }

    fn name(&self) -> &str {
        "ShortestPath"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortest_path_routing() {
        let mut topology = NetworkTopology::new();
        topology.add_link(1, 2, 1);
        topology.add_link(2, 3, 1);
        topology.add_link(1, 3, 10); // Longer path

        let mut policy = ShortestPathRouting::new();
        policy.update(&topology);

        // Path from 1 to 3 should go through 2
        let next_hop = policy.compute_next_hop(1, 3, &topology);
        assert_eq!(next_hop, Some(2));
    }

    #[test]
    fn test_topology_neighbors() {
        let mut topology = NetworkTopology::new();
        topology.add_link(1, 2, 1);
        topology.add_link(1, 3, 2);

        let neighbors = topology.get_neighbors(1);
        assert_eq!(neighbors.len(), 2);
    }
}
