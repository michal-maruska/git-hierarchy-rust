# discover-graph — mental model

*Living doc: maintained across the refactor rounds; current API names reflect the
`NeighborEnumerator`/`DiscoverableGraph` rename (round 2). History of rounds at the bottom.*

## 0. Words: vertices vs nodes (and friends)

Two registers of naming run through everything -- keep them apart:

- **vertex** — the *application-side* object: a concrete `T` value with meaning to the
  caller outside the graph world, e.g. a branch/segment name `String` ("seg2"), an `i32`
  in tests. Vertices are what the `NeighborEnumerator` talks about *and* what the caller
  thinks in; they are handed in as seeds and returned as neighbors.
- **node** — the *projection* of a vertex into the graph: a `NodeIndex`, just a number.
  The projection is the job of `get_or_create_node` (§3): given a vertex (a value that
  means something to you), produce/emit the integer that addresses this exact value inside
  the graph. petgraph algorithms -- DFS included -- only ever see nodes; the app only
  ever names vertices.

So: *the app speaks vertices; the algorithms speak nodes;* the discovered graph is the
bilingual body that holds the mapping both ways -- `NodeIndex`->vertex via the weights,
vertex->`NodeIndex` via the valueless index. Everything non-trivial (unification,
tops, label maps) crosses this boundary exactly once, through `get_or_create_node` /
`graph[i]`.

Other load-bearing words:

| word | meaning |
|---|---|
| universe | all vertices the enumerator can list (`known_vertices`): segments + sums |
| seed | a vertex dropped into the graph *before* traversal (`add_start_vertex`) |
| explosion (`explode_node`) | asking the enumerator for a node's neighbors and registering them as edges -- once ever per node |
| unification | two equal vertices map to the *one* node (Eq against the canonical weight; §3) |
| found-so-far | the graph-so-far as sole authority; enumerator answers are deltas merged into it (§1) |
| tops | vertices nobody derives from: incoming-degree 0 under child->parent edges (§7) |

## 1. Two passes: traversal vs explosion

Everything in the crate is one of these two passes. Hold only this distinction and the
indirections stop being confusing:

- **Traversal** — petgraph's generic `depth_first_search` walks `NodeIndex`es. It believes
  the graph is complete. It never talks to the external world. It uses a per-search visit
  map (`DynamicBitSet`) and, for each node it reaches, asks `neighbors(node)`.
- **Explosion** — the answer to `neighbors(node)` is *not* taken from edges: the first call
  on the iterator explodes the node, i.e. calls `NeighborEnumerator::get_neighbors` **once**,
  registers the result as edges/vertices into the graph, and *then* iterates the
  now-materialized edges.

petgraph "thinks" it's walking an ordinary graph; `DiscoverableGraph` makes that statement
true just-in-time. The graph "found-so-far" is the authoritative object store; enumeration
responses are incremental *deltas* merged into it.

## 2. The four actors

| Name | Role | At the exit door |
|---|---|---|
| `NeighborEnumerator<T>` (`src/neighbors_enumerator.rs`) | **THE external API** — user code answering "neighbors of this vertex?" on demand; may carry side-state (e.g. `GitHierarchyProvider.object_map`) | returned to the caller — it's the outside world's handle, *by design not droppable* |
| `GraphDiscoverer` (`src/graph_discoverer.rs`) | facade/session: owns the discoverable graph, runs DFS through petgraph's generic `depth_first_search` | consumed: one-shot `dfs_discover_graph` |
| `DiscoverableGraph<T, P>` (`src/discoverable_graph.rs`) | petgraph-trait shell (`GraphBase`/`Visitable`/`IntoNeighbors`) over a graph that **makes itself exist**: node explosion on first neighbor iteration | the take-home: its `graph` + `provider` are `into_inner`'d; scaffolding dies |
| `DiscoveringNeighborsIter` (`src/discoverable_graph.rs`) | the lazy iterator: first `.next()` explodes the node, only then walks edges | scaffolding (dropped with the wrapper) |

The driver surface (all of it):

```text
GraphDiscoverer::dfs_discover(&mut self, T) -> Vec<NodeIndex>   // incremental; feed more roots over time
GraphDiscoverer::dfs_discover_graph(self, T)                    // one-shot consuming exit
    -> (StableDiGraph<T, ()>, P, Vec<NodeIndex>)
GraphDiscoverer::get_dynamic_graph[_mut]() -> &DiscoverableGraph  // piecewise session access
GraphDiscoverer::discovered_count() / edges_count()
```

**The output is indices.** Vertex values (`T`) never leave the crate — they live exactly
once, in the graph's node weights. Any label→index map or readable order is a client-side
one-pass derivation from the returned graph (see §7).

## 3. The stitching point: `get_or_create_node`

Who unifies vertices found-so-far with newly discovered neighbors: `get_or_create_node`
(`discoverable_graph.rs`). The identity ledger is a **valueless index**:

```rust
vertex_to_node: RefCell<HashMap<u64 /*vertex-hash*/, Vec<NodeIndex>>>
```

Every neighbor the enumerator returns is pushed through:

- **Hit** → hash-bucket candidates are checked with `graph[i] == vertex` — equality is
  resolved *against the canonical weight the graph already holds*. The existing `NodeIndex`
  is reused and the edge attaches to the already-present node. Zero allocations, four bytes
  of bookkeeping material touched.
- **Miss** → the owned argument **moves into** `graph.add_node(vertex)` — the graph keeps
  the *only* copy — and the index records just `hash + NodeIndex`.

Unification is purely **semantic identity on `T`** (`Eq + Hash`): equal `T` ⇒ one node.
In the git case: `"master"` reached via two different summands resolves to one node;
but a symbolic ref and a direct ref naming the *same* commit are two nodes if their `T`
differs.

Cost per vertex:

| | hit | miss |
|---|---|---|
| round 0 (`entry(vertex.clone())` world) | 1 wasted deep clone | 2 deep allocs + 1 move |
| now (valueless index) | **0** | **0** (provider's original string, moved) |

## 4. Explosion orchestration: `explode_node`

Sequence for one node:

1. **Trigger** — `DiscoveringNeighborsIter::next()` calls `explode_node` on first access,
   then rebuilds its internal `WalkNeighbors` over the freshly populated list. This is what
   makes petgraph's DFS "discover on the fly": the neighbor iterator's first `.next()`
   explodes the node.
2. **Memoized once** — `discovered: RefCell<HashSet<NodeIndex>>` guarantees the enumerator
   is called **exactly once** per node, ever; afterwards the node is frozen (subsequent
   visits walk materialized edges). NodeIndex-keyed = identity, not value.
3. **No-copy call-out** — the enumerator is invoked with `&graph[node_idx]`: a borrow of
   the canonical weight. No `T` copy crosses into or out of the graph.
4. **Filter** — each returned neighbor passes `NeighborEnumerator::vertex_exists` before
   entering the index.
5. **Edge dedup** — `find_edge(node_idx, neighbor_node).is_none()` guards a delta re-listing
   the same neighbor.

## 5. Three separate "seen-before" sets (easy to confuse)

| Structure | Keyed by | Role |
|---|---|---|
| `vertex_to_node` | hash of `T` (valueless) | unify vertices across the whole discovery — *who exists in the graph* |
| `discovered` | `NodeIndex` (identity) | unify enumerator calls — *who has been exploded* |
| DFS visit map (`DynamicBitSet`) | `NodeIndex` | per-search state — *who DFS has touched* in this particular traversal |

`Discovered` being identity-keyed is correct *because of* unification: after stitching,
equal vertices are one node, so "per-node" ≡ "per-vertex".

## 6. Why the `T`s live only in the graph (and why not `&T`)

Goal: the graph is the single owner; ancillary structures shouldn't copy vertex values.
Safe-Rust route taken: a **valueless hash index** — the map stores no `T`, equality is
verified against the graph's own weights.

The literal "store `&T` keys pointed into an owned graph" is a three-layer wall, documented
for future readers:

1. **Borrow guard** — `RefCell` guards produce temporary lifetimes; a `HashMap<&'a T>`
   inside the same struct outlives every guard (E0597). No safe lifetime can exist.
2. **Weights move** — `StableDiGraph` keeps weights in `Vec<Option<Node<T>>>`; growth
   reallocates and moves them, so unboxed `&T`/`*const T` dangle on the next `add_node`.
   (Would require `Box<T>` weights forever.)
3. **Soundness discipline** — with `&'a T` (even `'static`-leaked) next to `&mut` access:
   every future weight rewrite / `remove_node` must purge index entries first, i.e. you
   hand-write the proof forever.

Middle ground rejected as unnecessary: `Rc<T>` sharing (same single allocation, zero
unsafe) — the valueless index is strictly leaner.

## 7. The exit doors and the client side

- `dfs_discover_graph(self, start)` — one shot: seed, run the DFS, and destructure the
  wrapper: `(graph.into_inner(), provider.into_inner(), order)`. `vertex_to_node` and
  `discovered` are dropped with the wrapper — by then they encode nothing the graph
  doesn't already show.
- `dfs_discover(&mut self, start)` — incremental: for piecewise discovery over multiple
  roots/session; returns traversal order as `Vec<NodeIndex>` (pushed on DFS `Finish`:
  post-order, preserved from round 0).

Client derivations (one pass over `graph.node_indices()`), which is why the library no
longer owns them: label→index maps, readable order.

**`src/graph/discover_pet.rs`** — bridges Git data: `GitHierarchyProvider` implements
`NeighborEnumerator<String>` by lazily calling `fetch_neighbors(vertex)`, which loads a
`GitHierarchy` for a branch/label from the repository and returns its parent/summand names.
`find_hierarchy` runs `dfs_discover_graph(root)` and assembles `HierarchyGraph`:
`graph` (indices→weights), `labeled_nodes`/`discovery_order` (client derivations), and
`labeled_objects = provider.object_map` — the enumerator's side-channel, which is part of
why the provider comes back out the door.

## 8. `NodeIndex` — the escape hatch from the borrow checker

The deeper pattern behind everything above: **we never stored references — we store numbers.**
`NodeIndex` is a handle relative to the graph; it carries no lifetime and addresses nothing
by itself, only through its owning `StableDiGraph` (row ID into a table, foreign key into a
DB, git SHA into the object store).

Two walls of safe Rust §6 died with that move, and a third bonus appeared:

1. **No lifetimes.** A number can be stored in any HashMap / DFS context / event closure
   without the E0597 problem; the "borrowed from itself" story that killed `&T` keys
   (§6 wall 1) never gets written.
2. **No interior-reference deadlock.** `depth_first_search(&self.dynamic_graph, …)` holds
   only a *shared* borrow of the wrapper while explosion reshapes the insides
   (`graph.borrow_mut()` inside `explode_node`). With real references into the graph the
   traversal's borrow would forbid just-in-time growth; transporting *nodes* (numbers)
   instead of *edges* makes "the DFS walks a graph that grows underneath it" legal.
   Shared-borrow traversal + `RefCell` interior mutation only composes because the
   traversal carries handles, not borrows.
3. **Closures get cheap.** The DFS event handler owns `Vec<NodeIndex> order`, not
   `Vec<&T>`: no lifetime annotation, no clone-protection dance.

The honest bill (discipline downgraded from compile-time to runtime):

- every "who exists?" question that `&T` provenance would answer for free is now answered
  by explicit machinery: the valueless index Eq-check against canonical weights (§3), the
  `discovered` memoization set (§4);
- validity of numeric handles is our obligation: we never `remove_node`, and
  `StableGraph` never renumbers or reuses slots after removal — the safest flavor of
  index-holding. Mass removal later would land us in generational-key territory
  (`slotmap`, `generational-arena`), with purge requirements exactly like §6 wall 3.

Same move well-trodden elsewhere: petgraph itself is index-built; ECS engines address
entities by id; `Rc`-free DAGs in Rust are, in practice, always arena + index. And the
analogy that fits this repository: git models its objects by SHA, never by pointer, into
an append-only store — `NodeIndex` into `StableDiGraph` is our mini-SHA system, and the
hash-keyed `vertex_to_node` is lookup by content into it.

## 9. Round history

- **Round 0** — `vertex_to_node: HashMap<T, NodeIndex>` via `entry(vertex.clone())`.
  std's `entry()` takes keys by value (no borrowed-key entry on stable — rust-lang/rust#60889;
  `hashbrown` has `entry_ref`), so every hit paid a wasted clone; every vertex lived in
  2–3 copies (map key, weight, `discovered` value). Encoded in this repo's earlier
  descriptions and in memo comments (`"why immediately clone()?"`, `"why is this from &T?"`).
- **Round 1** — valueless index: `HashMap<u64, Vec<NodeIndex>>` keyed by hash,
  Eq resolved against graph weights; `discovered` → `HashSet<NodeIndex>`; provider called
  with a borrow of the canonical weight. Zero `T` allocations on hits and misses.
- **Round 2** — naming follows roles: `NeighborEnumerator` (external API),
  `DiscoverableGraph` (self-extending graph), `DiscoveringNeighborsIter` (lazy iterator);
  traversal output is `Vec<NodeIndex>`; one consuming door `dfs_discover_graph`;
  `get_graph`/`get_provider`/`dfs_discover_iterator` removed; label maps became
  client-side derivations.
