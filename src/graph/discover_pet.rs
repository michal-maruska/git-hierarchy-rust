/**
PETgraph based

 Bridges Git data to the discovery engine:
 GitHierarchyProvider implements GraphProvider<String> by lazily calling fetch_neighbors(vertex), which loads a GitHierarchy
 for a branch/label from the repository and returns its parent/summand names.
 `find_hierarchy' runs the DFS discovery from a root and materializes the result as a petgraph StableGraph plus
 label→NodeIndex maps, recording discovery order.
*/
use crate::git_hierarchy::{GitHierarchy, load, SEGMENT_BASE_PATTERN};
use discover_graph::{GraphDiscoverer, NeighborEnumerator};

// GitHierarchy implements this, but we have to import explicitly.
// ^^^^^^^^^^^^^ method not found in `GitHierarchy<'_>`
use git2::Repository;

use std::collections::HashMap;

// Example with external data source
pub struct GitHierarchyProvider<'repo> {
    repository: &'repo Repository,
    pub object_map: HashMap<String, GitHierarchy<'repo>>,
    call_count: usize,
}

// What kind of structure do I want at the end?  .... (why?) topological order ... iterator? lookup? walk down?
// what operations do I want .... on the graph still?  Stitch/clone/replace nodes?

// can we have a hash  str () -> githierarchy?
//
// then the graph will handle str.

// Example with external data source

// hardcoded to use String as
impl<'repo> GitHierarchyProvider<'repo> {
    pub fn new(repo: &'repo Repository) -> Self {
        Self {
            repository: repo,
            object_map: HashMap::new(),
            call_count: 0,
        }
    }

    fn fetch_neighbors(&mut self, vertex: &String) -> Vec<String> {
        self.call_count += 1;
        // get from the object_map
        let repository = self.repository;
        let gh = match load(repository, vertex) {
            Ok(gh) => gh,
            Err(_) => return Vec::new(),
        };
        // convert if necessary

        // Get the children,
        let mut ch = Vec::new();

        match gh {
            // regular branch: a leaf/bottom of the hierarchy (no declared parents)
            GitHierarchy::Name(_) => {
                // was panic!("unprepared") -- poisoned whole-discovery sweeps:
                // sweeps legitimately reach plain branches via bases.
            }
            GitHierarchy::Segment(ref s) => {
                let symbolic_base = s.base(repository);
                // back to name -- in CANONICAL bare form: the universe
                // (segments()) yields bare names too, so both worlds
                // unify onto one node.
                let base_full = symbolic_base.name().unwrap(); // e.g. "refs/base/seg2"
                let bare = base_full.strip_prefix(SEGMENT_BASE_PATTERN).unwrap_or(base_full);
                ch.push(bare.to_owned());
            }
            GitHierarchy::Sum(ref s) => {
                // copy
                for summand in s.summands(repository) {
                    ch.push(summand.name().unwrap().to_owned());
                }
            }
            GitHierarchy::Reference(ref _r) => {
                // Vec::new()
            }
        }

        // let ch = gh.node_children();
        // this should be Strings

        // put in the object_map
        self.object_map.insert(vertex.to_owned(), gh);
        // return as Strings
        // convert vec<&str> to vec<String> ?
        ch
    }
}

impl<'repo> NeighborEnumerator<String> for GitHierarchyProvider<'repo> {
    fn get_neighbors(&mut self, vertex: &String) -> Vec<String> {
        // bug: std::thread::sleep(std::time::Duration::from_millis(10));
        self.fetch_neighbors(vertex)
    }

    /// The known universe: all segments and all sums (bare names, same
    /// canonical form `fetch_neighbors` pushes).  Enables
    /// `GraphDiscoverer::discover_whole_graph` ("find the whole graph,
    /// then look for the very top nodes").
    fn known_vertices(&mut self) -> Option<Vec<String>> {
        let mut all = Vec::new();
        if let Ok(it) = crate::git_hierarchy::segments(self.repository) {
            all.extend(it);
        }
        if let Ok(it) = crate::git_hierarchy::sums(self.repository) {
            all.extend(it);
        }
        Some(all) // Err read as "no such things" -- consistent with fetch's Err -> empty
    }

    /*
    fn vertex_exists(&mut self, vertex: &String) -> bool {
        // present in the object_map
        // !vertex.is_empty() && vertex.len() <= 10
        true
    }
    */
}


pub struct HierarchyGraph<'repo> {
    // name -> object
    pub labeled_objects: HashMap<String, GitHierarchy<'repo>>,
    // name->index
    pub labeled_nodes: HashMap<std::string::String, petgraph::stable_graph::NodeIndex>,
    //  on integers
    pub graph: petgraph::stable_graph::StableGraph<std::string::String, ()>,
}

pub fn find_hierarchy<'repo>(
    repo: &'repo Repository,
    root: String,
) -> (HierarchyGraph<'repo>, Vec<String>)
// GraphDiscoverer<String,GitHierarchyProvider<'repo>>
{
    // 1. Perform discovery, one-shot: everything comes back at once.
    // Indices out of the library; the strings (vertex values) live on the
    // returned graph and in the provider's object_map side-channel.
    let provider = GitHierarchyProvider::new(repo);
    let discoverer = GraphDiscoverer::new(provider); // consumed by the one-shot door

    let (graph, provider, order_nodes) = discoverer.dfs_discover_graph(root);

    let discovery_order: Vec<String> = order_nodes.into_iter().map(|i| graph[i].clone()).collect();

    (into_hierarchy(graph, provider),
     discovery_order)
}

/// The original task: discover the WHOLE graph (universe = all segments
/// and sums, seeded, then swept to fixpoint -- unification is the library's
/// job), then find the very TOP nodes: vertices nothing derives from.
///
/// Edges here are child -> parent, so "top" == incoming degree 0.
pub fn find_whole_hierarchy<'repo>(repo: &'repo Repository) -> (HierarchyGraph<'repo>, Vec<String>) {
    let provider = GitHierarchyProvider::new(repo);
    let discoverer = GraphDiscoverer::new(provider);

    let (graph, provider) = discoverer.discover_whole_graph();

    let tops: Vec<String> = top_nodes(&graph).iter().map(|&i| graph[i].clone()).collect();

    (into_hierarchy(graph, provider), tops)
}

/// tops of the hierarchy: vertices that are no one's base / summand-target:
/// no *incoming* edges under the child -> parent orientation.
pub fn top_nodes(graph: &petgraph::stable_graph::StableGraph<String, ()>)
    -> Vec<petgraph::stable_graph::NodeIndex>
{
    graph
        .node_indices()
        .filter(|&n| graph.neighbors_directed(n, petgraph::Direction::Incoming).next().is_none())
        .collect()
}

/// Client-side assembly, shared by the one-root and whole-graph doors:
/// label maps derived in one pass from the graph (which owns the strings).
fn into_hierarchy<'repo>(
    graph: petgraph::stable_graph::StableGraph<String, ()>,
    provider: GitHierarchyProvider<'repo>
) -> HierarchyGraph<'repo> {
    // label map + readable order are derived from the weights BEFORE the
    // graph is moved into its field
    let labeled_nodes: HashMap<String, petgraph::stable_graph::NodeIndex> = graph
        .node_indices()
        .map(|i| (graph[i].clone(), i))
        .collect();

    // we cannot drop the provider wholesale: its object_map
    // is the only place the GitHierarchy objects live.
    HierarchyGraph {
        labeled_objects: provider.object_map, // String -> GitHierarchy
        labeled_nodes,
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::stable_graph::StableDiGraph;

    // edges are child -> parent:  top == no incoming edges
    #[test]
    fn test_top_nodes() {
        let mut g: StableDiGraph<String, ()> = StableDiGraph::new();
        let feature = g.add_node("feature".to_string()); // top: nobody builds on it
        let mid = g.add_node("mid".to_string());
        let master = g.add_node("master".to_string()); // bottom: no base of its own
        g.add_edge(feature, mid, ()); // feature's base is mid
        g.add_edge(mid, master, ()); // mid's base is master
        assert_eq!(top_nodes(&g), vec![feature]);
        // and bottoms are the out-degree-0 nodes:
        assert_eq!(
            g.node_indices()
                .filter(|&n| g.neighbors_directed(n, petgraph::Direction::Outgoing).next().is_none())
                .collect::<Vec<_>>(),
            vec![master]
        );
    }
}
