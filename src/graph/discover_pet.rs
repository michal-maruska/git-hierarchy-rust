/**
PETgraph based

 Bridges Git data to the discovery engine:
 GitHierarchyProvider implements GraphProvider<String> by lazily calling fetch_neighbors(vertex), which loads a GitHierarchy
 for a branch/label from the repository and returns its parent/summand names.
 `find_hierarchy' runs the DFS discovery from a root and materializes the result as a petgraph StableGraph plus
 label→NodeIndex maps, recording discovery order.
*/
use crate::git_hierarchy::{GitHierarchy, load, SEGMENT_BASE_PATTERN};
use crate::utils::extract_name;
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

/// THE canonical bare name inside the graph: every edge name and every
/// universe entry speak this one dialect, or unification duplicates
/// nodes (segments looked like tops of their own full-name copies).
/// - refs/base/seg2       -> "seg2"      (the universe segments() form)
/// - refs/heads/component -> "component" (extract_name)
/// - refs/tags/v1         -> "v1"
/// The reverse direction (load(bare)) resolves again: dwim tries
/// refs/{name} first, then refs/tags/, refs/heads/ ... -- "seg2"
/// resolves through the segment's own head branch, which
/// Segment::create also creates; sums resolve through their
/// refs/heads/{sum} branch made by Sum::create.
fn canonical_bare_name(refname: &str) -> String {
    match refname.strip_prefix(SEGMENT_BASE_PATTERN) {
        Some(bare) => bare.to_owned(),
        None => extract_name(refname).to_owned(),
    }
}

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
                // symbolic target's FULL name -- canonicalize to the bare
                // form the universe speaks, so the edge lands on the
                // same node the universe seeded.
                let base_full = symbolic_base.name().unwrap(); // e.g. "refs/base/seg2"
                ch.push(canonical_bare_name(base_full));
            }
            GitHierarchy::Sum(ref s) => {
                // summands' symbolic targets resolve to FULL reference
                // names (refs/base/seg1, refs/heads/component).
                // canonicalize: bare names are the one dialect the
                // universe (segments()/sums()) uses -- full spellings
                // duplicated every summand as an extra node and made the
                // bare segment look like a top.
                for summand in s.summands(repository) {
                    let summand_full = summand.name().unwrap();
                    ch.push(canonical_bare_name(summand_full));
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
    use crate::test_utils::TestRepo;
    use crate::graph::discover::NodeExpander;
    use crate::git_hierarchy::{GitHierarchy, Segment, Sum};

    // Hierarchical shape == the user's world:
    //   main <- seg1 <- seg2,  sum1 = seg1 + seg2
    // Child -> parent edges; tops = incoming 0.  Regression for the bug
    // "tops contained every segment": sums pushed FULL summand refs
    // (refs/base/seg1), not the universe's bare names.
    #[test]
    fn test_whole_graph_unifies_bare_names() {
        let repo = TestRepo::new();
        let _initial = repo.create_initial_commit_on_main();
        let _seg1 = repo.generate_sample_segment("main", "seg1", "f1");
        // chained over seg1: refs/heads/seg1 exists (Segment::create made it),
        // so the short name resolves; the symbolic base ref is refs/base/seg1.
        let _seg2 = repo.generate_sample_segment("seg1", "seg2", "f2");
        let _sum = repo.create_sample_sum("sum1", &["seg1", "seg2"]);

        let (hierarchy_graph, tops) = find_whole_hierarchy(&repo.repo);
        let nodes: Vec<String> = hierarchy_graph.graph.node_weights().cloned().collect();
        eprintln!("nodes: {:?}", nodes);

        // Every crossing has been pushed in bare, canonical form:
        // universe gave seg1/seg2/sum1; seg1's base is plain "main"
        // (extract_name); seg2's base is bare "seg1"; sum1's summands
        // are bare "seg1"/"seg2" -- all unified onto 4 nodes.
        assert_eq!(nodes.len(), 4, "names must unify: {}", nodes.len());

        // 4 edges: seg1->main, seg2->seg1, sum1->seg1, sum1->seg2
        assert_eq!(hierarchy_graph.graph.edge_count(), 4);

        // only the sum lies on top: nothing derives FROM it
        assert_eq!(tops, vec!["sum1".to_owned()], "unexpected tops: {:?}", tops);
    }

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
