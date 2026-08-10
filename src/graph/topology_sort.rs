use std::collections::VecDeque;

/// Performs topological sort on a directed graph using Kahn's algorithm
///
/// # Arguments
/// * `graph` - Adjacency list where graph[i] contains vertices that vertex i points to
///
/// # Returns
/// * `Some(Vec<usize>)` - Topologically sorted order if graph is acyclic
/// * `None` - If graph contains a cycle
pub fn topological_sort(graph: &[Vec<usize>]) -> Option<Vec<usize>> {
    let n = graph.len();

    // Calculate in-degrees for each vertex
    let mut in_degree = vec![0; n];
    for (_node_idx, neighbors) in graph.iter().enumerate() {
        for &v in neighbors {
            if v < n {
                // Bounds check
                in_degree[v] += 1;
            }
        }
    }

    // Initialize queue with vertices having in-degree 0
    let mut queue = VecDeque::new();
    for (i, degree) in in_degree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(i);
        }
    }

    let mut result = Vec::new();

    // Process vertices in topological order
    while let Some(u) = queue.pop_front() {
        result.push(u);

        // Decrease in-degree of adjacent vertices
        for &v in &graph[u] {
            if v < n {
                // Bounds check
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    queue.push_back(v);
                }
            }
        }
    }

    // Check if all vertices were processed (no cycles)
    if result.len() == n {
        Some(result)
    } else {
        None // Cycle detected
    }
}

/// Alternative implementation using DFS-based approach
pub fn topological_sort_dfs(graph: &Vec<Vec<usize>>) -> Option<Vec<usize>> {
    let n = graph.len();
    let mut visited = vec![false; n];
    let mut rec_stack = vec![false; n];
    let mut result = Vec::new();

    fn dfs(
        graph: &Vec<Vec<usize>>,
        u: usize,
        visited: &mut Vec<bool>,
        rec_stack: &mut Vec<bool>,
        result: &mut Vec<usize>,
    ) -> bool {
        visited[u] = true;
        rec_stack[u] = true;

        for &v in &graph[u] {
            if v >= graph.len() {
                continue; // Skip invalid vertices
            }

            if rec_stack[v] {
                return false; // Back edge found - cycle detected
            }

            if !visited[v] && !dfs(graph, v, visited, rec_stack, result) {
                return false;
            }
        }

        rec_stack[u] = false;
        result.push(u);
        true
    }

    // Visit all vertices
    for i in 0..n {
        if !visited[i] && !dfs(graph, i, &mut visited, &mut rec_stack, &mut result) {
            return None; // Cycle detected
        }
    }

    // Reverse to get correct topological order
    result.reverse();
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dag() {
        // Graph: 0 -> 1 -> 2
        //        0 -> 2
        let graph = vec![
            vec![1, 2], // 0 points to 1 and 2
            vec![2],    // 1 points to 2
            vec![],     // 2 points to nothing
        ];

        let result = topological_sort(&graph).unwrap();
        assert_eq!(result, vec![0, 1, 2]);

        let result_dfs = topological_sort_dfs(&graph).unwrap();
        // DFS may produce different valid order
        assert!(is_valid_topological_order(&graph, &result_dfs));
    }

    #[test]
    fn test_cycle_detection() {
        // Graph with cycle: 0 -> 1 -> 2 -> 0
        let graph = vec![
            vec![1], // 0 -> 1
            vec![2], // 1 -> 2
            vec![0], // 2 -> 0 (creates cycle)
        ];

        assert!(topological_sort(&graph).is_none());
        assert!(topological_sort_dfs(&graph).is_none());
    }

    #[test]
    fn test_disconnected_components() {
        // Two disconnected components: 0->1 and 2->3
        let graph = vec![
            vec![1], // 0 -> 1
            vec![],  // 1 -> nothing
            vec![3], // 2 -> 3
            vec![],  // 3 -> nothing
        ];

        let result = topological_sort(&graph).unwrap();
        assert!(is_valid_topological_order(&graph, &result));
    }

    fn is_valid_topological_order(graph: &[Vec<usize>], order: &[usize]) -> bool {
        let mut position = vec![0; graph.len()];
        for (i, &vertex) in order.iter().enumerate() {
            position[vertex] = i;
        }

        for u in 0..graph.len() {
            for &v in &graph[u] {
                if v < graph.len() && position[u] >= position[v] {
                    return false;
                }
            }
        }
        true
    }
}
