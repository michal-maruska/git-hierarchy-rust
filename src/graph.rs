pub mod discover;
pub mod discover_pet;
pub mod topology_sort;
use crate::graph::topology_sort::topological_sort;

type Range = usize;
pub struct Graph {
    vertices: usize,
    adjacency_list: Vec<Vec<Range>>,
}

// clippy suggestion:
impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            vertices: 0,
            adjacency_list: Vec::new(),
        }
    }

    pub fn add_vertices(&mut self, n: usize) {
        if n > self.vertices {
            self.vertices = n;
        }

        // reserve:
        let list = &mut self.adjacency_list;
        if list.len() <= n {
            list.resize(n + 1, Vec::new());
            list.resize_with(n + 1, Vec::new);
        }
    }

    pub fn add_edge(&mut self, from: Range, to: Range) {
        // Index::index_mut(self.adjacency_list,from);
        let list = &mut self.adjacency_list;

        // list.get(from);
        list[from].push(to);
    }

    pub fn toposort(&self) -> Vec<usize> {
        let matrix = &self.adjacency_list;

        if let Some(order) = topological_sort(matrix) {
            // println!("found order {:?}", order);
            order
        } else {
            panic!("bad topo order");
        }
    }

    pub fn dump_graph(&self) {
        println!("Graph of {} vertices:", self.vertices);
        let matrix = &self.adjacency_list;
        for (index, row) in matrix.iter().enumerate() {
            print!("{}:", index);
            for edge in row {
                print!("{}", edge);
            }
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_new_and_default() {
        let g1 = Graph::new();
        assert_eq!(g1.vertices, 0);

        let g2 = Graph::default();
        assert_eq!(g2.vertices, 0);
    }

    #[test]
    fn test_graph_toposort() {
        let mut g = Graph::new();
        g.add_vertices(2); // indices 0, 1, 2
        g.add_edge(0, 1);
        g.add_edge(1, 2);

        let order = g.toposort();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    #[should_panic(expected = "bad topo order")]
    fn test_graph_toposort_cycle_panics() {
        let mut g = Graph::new();
        g.add_vertices(2);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        g.add_edge(2, 0);

        g.toposort();
    }
}
