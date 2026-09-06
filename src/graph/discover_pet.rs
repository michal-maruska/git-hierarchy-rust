use crate::git_hierarchy::{GitHierarchy, load};
use discover_graph::{GraphDiscoverer, GraphProvider};

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
            // regular branch. say `master'
            GitHierarchy::Name(_x) => {
                panic!("unprepared")
            }
            GitHierarchy::Segment(ref s) => {
                let symbolic_base = s.base(repository);
                // back to name...
                ch.push(symbolic_base.name().unwrap().to_owned());
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

impl<'repo> GraphProvider<String> for GitHierarchyProvider<'repo> {
    fn get_neighbors(&mut self, vertex: &String) -> Vec<String> {
        // bug: std::thread::sleep(std::time::Duration::from_millis(10));
        self.fetch_neighbors(vertex)
    }

    /*
    fn vertex_exists(&mut self, vertex: &String) -> bool {
        // present in the object_map
        // !vertex.is_empty() && vertex.len() <= 10
        true
    }
    */
}

//
pub struct HierarchyGraph<'repo> {
    pub labeled_objects: HashMap<String, GitHierarchy<'repo>>,
    // label->index
    pub labeled_nodes: HashMap<std::string::String, petgraph::stable_graph::NodeIndex>,
    pub graph: petgraph::stable_graph::StableGraph<std::string::String, ()>,
    // order labels
    pub discovery_order: Vec<std::string::String>,
}

pub fn find_hierarchy<'repo>(
    repo: &'repo Repository,
    root: String,
) -> HierarchyGraph<'repo>
// GraphDiscoverer<String,GitHierarchyProvider<'repo>>
{
    // 1. Perform discovery
    // not `mut' ?
    let provider = GitHierarchyProvider::new(repo);
    let mut discoverer = GraphDiscoverer::new(provider); // consumes!

    // T P ... P is provider, T String?
    // GraphProvider<String>
    // So we work with strings. Then ... how do we map to GitHierarchy ?
    let discovery_order = discoverer.dfs_discover(root);

    let graph = discoverer.get_graph();
    // is this different from provider ?
    let (provider, hash_to_graph) = discoverer.get_provider();

    // we cannot drop the provider.
    // but we can move out of it?
    HierarchyGraph {
        labeled_objects: provider.object_map, // String -> GitHierarchy
        labeled_nodes: hash_to_graph,       // stable graph:  String -> index ?
        graph,               // index -> String?
        discovery_order,
    } //  indices?
}
