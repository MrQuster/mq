#![deny(clippy::await_holding_lock)]
//! Explicit Dependency DAG planner (`mq plan`) and concurrent async executor.
//!
//! Provides static topological wave decomposition, exact longest-path critical
//! path calculation via dynamic programming, theoretical parallelism analysis,
//! human-readable UTF-8 plan formatting (Lesson E9), and an event-driven
//! `tokio::task::JoinSet` executor integrated with [`CancelEpoch`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{self, Write as _};
use std::future::Future;
use std::pin::Pin;

use tokio::task::JoinSet;

use crate::epoch::{CancelEpoch, EpochGuard};

/// Errors produced during DAG validation or async execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    /// Two nodes were registered with the same `id`.
    DuplicateNode(String),
    /// A node declared a dependency `missing_dep` that does not exist in the DAG.
    MissingDependency {
        /// Dependent node ID.
        node: String,
        /// Unknown upstream dependency ID.
        missing_dep: String,
    },
    /// A directed cycle was detected; contains the cycle path (`a -> b -> a`).
    CycleDetected(Vec<String>),
    /// Execution was cancelled at `epoch`.
    Cancelled {
        /// Cancelled epoch number.
        epoch: u64,
    },
    /// A DAG node task returned `Err(error)`.
    NodeFailed {
        /// Failing node ID.
        node: String,
        /// Error message returned by the node closure.
        error: String,
    },
    /// A DAG node task panicked inside `JoinSet`.
    NodePanicked {
        /// JoinError description.
        error: String,
    },
}

impl fmt::Display for DagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNode(id) => write!(f, "duplicate DAG node id: '{id}'"),
            Self::MissingDependency { node, missing_dep } => {
                write!(
                    f,
                    "DAG node '{node}' depends on non-existent node '{missing_dep}'"
                )
            }
            Self::CycleDetected(path) => {
                write!(f, "cycle detected in DAG: {}", path.join(" -> "))
            }
            Self::Cancelled { epoch } => {
                write!(f, "DAG execution cancelled at epoch {epoch}")
            }
            Self::NodeFailed { node, error } => {
                write!(f, "DAG node '{node}' failed: {error}")
            }
            Self::NodePanicked { error } => {
                write!(f, "DAG node task panicked: {error}")
            }
        }
    }
}

impl std::error::Error for DagError {}

/// Static metadata specification for a single stage/node in [`DagPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagNodeSpec {
    /// Unique identifier of the DAG node (e.g. `"download_assets"`).
    pub id: String,
    /// Human-readable stage description (e.g. `"Download Assets (Objects)"`).
    pub stage: String,
    /// Upstream node IDs that must complete before this node can start.
    pub deps: Vec<String>,
    /// Degree of internal item parallelism (e.g. 3800 assets, or `1` if sequential).
    pub parallel_items: usize,
    /// Estimated wall-clock duration of this node in milliseconds.
    pub estimated_ms: u64,
    /// Explicit architectural explanation when a stage must run sequentially (`Lesson E9`).
    pub sequential_reason: Option<String>,
}

impl DagNodeSpec {
    /// Constructs a new [`DagNodeSpec`] with default `parallel_items = 1` and no dependencies.
    #[must_use]
    pub fn new(id: impl Into<String>, stage: impl Into<String>, estimated_ms: u64) -> Self {
        Self {
            id: id.into(),
            stage: stage.into(),
            deps: Vec::new(),
            parallel_items: 1,
            estimated_ms,
            sequential_reason: None,
        }
    }

    /// Builder method setting upstream dependency IDs.
    #[must_use]
    pub fn with_deps(mut self, deps: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.deps = deps.into_iter().map(Into::into).collect();
        self
    }

    /// Builder method setting `parallel_items`.
    #[must_use]
    pub fn with_parallel_items(mut self, parallel_items: usize) -> Self {
        self.parallel_items = parallel_items;
        self
    }

    /// Builder method documenting why a node is sequential (`Lesson E9`).
    #[must_use]
    pub fn with_sequential_reason(mut self, reason: impl Into<String>) -> Self {
        self.sequential_reason = Some(reason.into());
        self
    }
}

/// Validated, acyclic execution plan supporting wave decomposition, exact
/// longest-path critical path DP, and `mq plan` reporting.
#[derive(Debug, Clone)]
pub struct DagPlan {
    nodes: Vec<DagNodeSpec>,
    index_by_id: HashMap<String, usize>,
    waves: Vec<Vec<String>>,
    topo_order: Vec<String>,
}

struct ValidatedTopo {
    index_by_id: HashMap<String, usize>,
    clean_deps: Vec<Vec<String>>,
    waves: Vec<Vec<String>>,
    topo_order: Vec<String>,
}

/// Validates node IDs, dependencies, and acyclicity, returning topological metadata.
fn validate_and_toposort(
    ids_and_deps: &[(String, Vec<String>)],
) -> Result<ValidatedTopo, DagError> {
    let n = ids_and_deps.len();
    let mut index_by_id = HashMap::with_capacity(n);
    for (idx, (id, _)) in ids_and_deps.iter().enumerate() {
        if index_by_id.insert(id.clone(), idx).is_some() {
            return Err(DagError::DuplicateNode(id.clone()));
        }
    }

    let mut clean_deps: Vec<Vec<String>> = Vec::with_capacity(n);
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for (idx, (id, raw_deps)) in ids_and_deps.iter().enumerate() {
        let mut seen = HashSet::with_capacity(raw_deps.len());
        let mut deduped = Vec::with_capacity(raw_deps.len());
        for dep in raw_deps {
            if dep == id {
                return Err(DagError::CycleDetected(vec![id.clone(), id.clone()]));
            }
            let Some(&dep_idx) = index_by_id.get(dep) else {
                return Err(DagError::MissingDependency {
                    node: id.clone(),
                    missing_dep: dep.clone(),
                });
            };
            if seen.insert(dep_idx) {
                deduped.push(dep.clone());
                dependents[dep_idx].push(idx);
                in_degree[idx] += 1;
            }
        }
        clean_deps.push(deduped);
    }

    // Kahn's algorithm grouped by synchronous topological waves.
    let mut current_wave: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut waves: Vec<Vec<String>> = Vec::new();
    let mut topo_order: Vec<String> = Vec::with_capacity(n);

    while !current_wave.is_empty() {
        let mut next_wave: Vec<usize> = Vec::new();
        let mut wave_ids: Vec<String> = Vec::with_capacity(current_wave.len());

        for &u in &current_wave {
            let id = ids_and_deps[u].0.clone();
            wave_ids.push(id.clone());
            topo_order.push(id);

            for &v in &dependents[u] {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    next_wave.push(v);
                }
            }
        }

        waves.push(wave_ids);
        current_wave = next_wave;
    }

    if topo_order.len() != n {
        let cycle = extract_cycle(ids_and_deps, &index_by_id, &clean_deps);
        return Err(DagError::CycleDetected(cycle));
    }

    Ok(ValidatedTopo {
        index_by_id,
        clean_deps,
        waves,
        topo_order,
    })
}

/// Finds a deterministic directed cycle path using 3-color DFS over `dependents` / `deps`.
fn extract_cycle(
    ids_and_deps: &[(String, Vec<String>)],
    index_by_id: &HashMap<String, usize>,
    clean_deps: &[Vec<String>],
) -> Vec<String> {
    // Follow dependency edges u -> dep (if u depends on dep and there is a cycle,
    // reversing the cycle gives execution order dep -> ... -> u -> dep).
    let n = ids_and_deps.len();
    let mut color = vec![0u8; n]; // 0 = unvisited, 1 = visiting, 2 = visited
    let mut stack: Vec<usize> = Vec::new();

    fn dfs(
        u: usize,
        clean_deps: &[Vec<String>],
        index_by_id: &HashMap<String, usize>,
        color: &mut [u8],
        stack: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        color[u] = 1;
        stack.push(u);
        for dep_id in &clean_deps[u] {
            let v = index_by_id[dep_id];
            if color[v] == 1 {
                let pos = stack.iter().position(|&x| x == v).unwrap_or(0);
                let mut cycle: Vec<usize> = stack[pos..].to_vec();
                cycle.push(v);
                // Reverse so arrows represent prerequisite -> dependent execution flow.
                cycle.reverse();
                return Some(cycle);
            } else if color[v] == 0 {
                if let Some(found) = dfs(v, clean_deps, index_by_id, color, stack) {
                    return Some(found);
                }
            }
        }
        stack.pop();
        color[u] = 2;
        None
    }

    for start in 0..n {
        if color[start] == 0 {
            if let Some(indices) = dfs(start, clean_deps, index_by_id, &mut color, &mut stack) {
                return indices
                    .into_iter()
                    .map(|i| ids_and_deps[i].0.clone())
                    .collect();
            }
        }
    }

    Vec::new()
}

impl DagPlan {
    /// Validates `nodes` for uniqueness, dependency existence, and acyclicity,
    /// computing topological execution waves.
    pub fn new(mut nodes: Vec<DagNodeSpec>) -> Result<Self, DagError> {
        let ids_and_deps: Vec<(String, Vec<String>)> = nodes
            .iter()
            .map(|n| (n.id.clone(), n.deps.clone()))
            .collect();

        let ValidatedTopo {
            index_by_id,
            clean_deps,
            waves,
            topo_order,
        } = validate_and_toposort(&ids_and_deps)?;

        for (node, deduped) in nodes.iter_mut().zip(clean_deps) {
            node.deps = deduped;
        }

        Ok(Self {
            nodes,
            index_by_id,
            waves,
            topo_order,
        })
    }

    /// Returns all node specifications in registration order.
    #[must_use]
    pub fn nodes(&self) -> &[DagNodeSpec] {
        &self.nodes
    }

    /// Looks up a [`DagNodeSpec`] by ID in O(1) time.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<&DagNodeSpec> {
        self.index_by_id.get(id).map(|&idx| &self.nodes[idx])
    }

    /// Returns synchronous topological waves (`Wave 0`, `Wave 1`, ...).
    ///
    /// Every node in `waves()[w]` depends only on nodes in earlier waves `< w`
    /// and can execute concurrently.
    #[must_use]
    pub fn waves(&self) -> &[Vec<String>] {
        &self.waves
    }

    /// Returns a valid topological ordering of all node IDs.
    #[must_use]
    pub fn topological_order(&self) -> &[String] {
        &self.topo_order
    }

    /// Sum of `estimated_ms` across all nodes (`T_1` total DAG work).
    #[must_use]
    pub fn total_estimated_ms(&self) -> u64 {
        self.nodes.iter().map(|n| n.estimated_ms).sum()
    }

    /// Computes the exact critical path `(node_ids, duration_ms)` (`T_inf` span)
    /// using longest-path dynamic programming over the topological ordering.
    #[must_use]
    pub fn critical_path(&self) -> (Vec<String>, u64) {
        let n = self.nodes.len();
        if n == 0 {
            return (Vec::new(), 0);
        }

        let mut dist = vec![0u64; n];
        let mut pred: Vec<Option<usize>> = vec![None; n];

        for id in &self.topo_order {
            let u = self.index_by_id[id];
            let node = &self.nodes[u];

            let mut best_dep: Option<usize> = None;
            let mut best_dep_dist = 0u64;

            for dep_id in &node.deps {
                let dep_idx = self.index_by_id[dep_id];
                let candidate = dist[dep_idx];
                if best_dep.is_none() || candidate > best_dep_dist {
                    best_dep = Some(dep_idx);
                    best_dep_dist = candidate;
                }
            }

            pred[u] = best_dep;
            dist[u] = best_dep_dist.saturating_add(node.estimated_ms);
        }

        // Select sink with maximum cumulative duration (deterministic tie-break by registration index).
        let mut best_end = 0usize;
        for i in 1..n {
            if dist[i] > dist[best_end] {
                best_end = i;
            }
        }

        let total_crit_ms = dist[best_end];
        let mut path_indices = VecDeque::new();
        let mut cur = Some(best_end);
        while let Some(idx) = cur {
            path_indices.push_front(idx);
            cur = pred[idx];
        }

        let path = path_indices
            .into_iter()
            .map(|idx| self.nodes[idx].id.clone())
            .collect();

        (path, total_crit_ms)
    }

    /// Theoretical DAG parallelism (`T_1 / T_inf`), i.e. total estimated work
    /// divided by critical-path span.
    #[must_use]
    pub fn theoretical_parallelism(&self) -> f64 {
        if self.nodes.is_empty() {
            return 0.0;
        }
        let total = self.total_estimated_ms();
        let (_, crit) = self.critical_path();
        if crit == 0 {
            return self.waves.iter().map(Vec::len).max().unwrap_or(1) as f64;
        }
        total as f64 / crit as f64
    }

    /// Fraction `[0.0, 1.0]` of estimated work belonging to data-parallel or
    /// multi-branch concurrent stages (useful for Amdahl's Law reporting).
    #[must_use]
    pub fn parallel_fraction(&self) -> f64 {
        let total = self.total_estimated_ms();
        if total == 0 {
            return 0.0;
        }

        let mut concurrent_node_ids = HashSet::new();
        for wave in &self.waves {
            if wave.len() > 1 {
                for id in wave {
                    concurrent_node_ids.insert(id.as_str());
                }
            }
        }

        let parallel_ms: u64 = self
            .nodes
            .iter()
            .filter(|n| {
                (n.parallel_items > 1 && n.sequential_reason.is_none())
                    || concurrent_node_ids.contains(n.id.as_str())
            })
            .map(|n| n.estimated_ms)
            .sum();

        (parallel_ms as f64 / total as f64).clamp(0.0, 1.0)
    }

    /// Formats a crisp human-readable UTF-8 DAG + Critical Path report for `mq plan <version>`.
    #[must_use]
    pub fn format_plan(&self) -> String {
        let mut out = String::new();
        let (crit_path, crit_ms) = self.critical_path();
        let crit_set: HashSet<&str> = crit_path.iter().map(String::as_str).collect();
        let total_ms = self.total_estimated_ms();
        let parallelism = self.theoretical_parallelism();
        let par_fraction = self.parallel_fraction();

        let _ = writeln!(
            out,
            "DAG Execution Plan ({} nodes, {} waves)",
            self.nodes.len(),
            self.waves.len()
        );
        let _ = writeln!(
            out,
            "══════════════════════════════════════════════════════════════"
        );

        for (wave_idx, wave) in self.waves.iter().enumerate() {
            let wave_span_ms = wave
                .iter()
                .filter_map(|id| self.node(id))
                .map(|n| n.estimated_ms)
                .max()
                .unwrap_or(0);
            let mode = if wave.len() > 1 {
                "concurrent stage wave"
            } else {
                "single-stage wave"
            };
            let _ = writeln!(out, "Wave {wave_idx} ({mode}, max {wave_span_ms} ms):");

            for (pos, node_id) in wave.iter().enumerate() {
                let is_last = pos + 1 == wave.len();
                let branch = if is_last { "└──" } else { "├──" };
                let cont = if is_last { "   " } else { "│  " };
                let spec = &self.nodes[self.index_by_id[node_id]];
                let crit_badge = if crit_set.contains(spec.id.as_str()) {
                    "[CRIT] "
                } else {
                    ""
                };
                let deps_label = if spec.deps.is_empty() {
                    "root".to_string()
                } else {
                    format!("deps: [{}]", spec.deps.join(", "))
                };
                let items_label = if spec.parallel_items > 1 {
                    format!("parallel x{}", spec.parallel_items)
                } else {
                    "sequential (1 item)".to_string()
                };

                let _ = writeln!(
                    out,
                    "  {branch} {crit_badge}{} — {} ({} ms, {items_label}, {deps_label})",
                    spec.id, spec.stage, spec.estimated_ms
                );

                if let Some(ref reason) = spec.sequential_reason {
                    let _ = writeln!(out, "  {cont}    reason (sequential): {reason}");
                }
            }
        }

        let _ = writeln!(
            out,
            "──────────────────────────────────────────────────────────────"
        );
        if crit_path.is_empty() {
            let _ = writeln!(out, "Critical Path       : (empty) (0 ms)");
        } else {
            let _ = writeln!(
                out,
                "Critical Path       : {} ({crit_ms} ms)",
                crit_path.join(" -> ")
            );
        }
        let _ = writeln!(out, "Total Work (T_1)    : {total_ms} ms");
        let _ = writeln!(out, "Critical Span (T_∞) : {crit_ms} ms");
        let _ = writeln!(out, "DAG Parallelism     : {parallelism:.2}x");
        let _ = writeln!(
            out,
            "Parallelizable Work : {:.1}% (p = {:.3})",
            par_fraction * 100.0,
            par_fraction
        );

        out
    }
}

/// Pinned boxed future returned by an [`ExecutableDag`] node closure.
pub type BoxedNodeFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// Boxed async task closure taking an [`EpochGuard`] and returning a [`BoxedNodeFuture`].
pub type DagTaskFn = Box<dyn FnOnce(EpochGuard) -> BoxedNodeFuture + Send>;

/// Single executable node definition for [`ExecutableDag`].
pub struct ExecutableNode {
    /// Unique identifier of this node.
    pub id: String,
    /// Upstream dependency node IDs.
    pub deps: Vec<String>,
    /// Async task factory invoked once all `deps` have succeeded.
    pub task: DagTaskFn,
}

impl fmt::Debug for ExecutableNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutableNode")
            .field("id", &self.id)
            .field("deps", &self.deps)
            .finish_non_exhaustive()
    }
}

/// Event-driven concurrent async DAG runner backed by [`tokio::task::JoinSet`].
///
/// Spawns every dependency-ready node immediately as soon as its upstream
/// prerequisites complete (without stalling fast branches on unrelated slow
/// siblings), aborts all running tasks immediately on [`CancelEpoch`] advance,
/// and propagates the first error deterministically.
#[derive(Debug, Default)]
pub struct ExecutableDag {
    nodes: Vec<ExecutableNode>,
}

/// Alias matching architecture overview for [`ExecutableDag`].
pub type DagExecutor = ExecutableDag;

impl ExecutableDag {
    /// Creates an empty [`ExecutableDag`].
    #[must_use]
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Constructs an [`ExecutableDag`] from pre-built [`ExecutableNode`] items,
    /// validating acyclicity and dependencies eagerly.
    pub fn from_nodes(nodes: Vec<ExecutableNode>) -> Result<Self, DagError> {
        let dag = Self { nodes };
        dag.validate()?;
        Ok(dag)
    }

    /// Registers a node with a boxed [`DagTaskFn`] closure.
    pub fn add_node(
        &mut self,
        id: impl Into<String>,
        deps: impl IntoIterator<Item = impl Into<String>>,
        task: DagTaskFn,
    ) -> &mut Self {
        self.nodes.push(ExecutableNode {
            id: id.into(),
            deps: deps.into_iter().map(Into::into).collect(),
            task,
        });
        self
    }

    /// Ergonomic helper registering an async closure without manual `Box::pin` boilerplate.
    pub fn add_async_node<F, Fut>(
        &mut self,
        id: impl Into<String>,
        deps: impl IntoIterator<Item = impl Into<String>>,
        task: F,
    ) -> &mut Self
    where
        F: FnOnce(EpochGuard) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.add_node(id, deps, Box::new(move |guard| Box::pin(task(guard))))
    }

    /// Number of nodes registered in this DAG.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if no nodes are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Validates uniqueness, dependency existence, and acyclicity, returning
    /// the static topological waves.
    pub fn validate(&self) -> Result<Vec<Vec<String>>, DagError> {
        let ids_and_deps: Vec<(String, Vec<String>)> = self
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.deps.clone()))
            .collect();
        let validated = validate_and_toposort(&ids_and_deps)?;
        Ok(validated.waves)
    }

    /// Snapshots `cancel_epoch` and executes all DAG nodes concurrently as
    /// dependencies become ready, returning the completion order of node IDs.
    pub async fn execute(self, cancel_epoch: &CancelEpoch) -> Result<Vec<String>, DagError> {
        let guard = cancel_epoch.snapshot();
        self.execute_with_guard(guard).await
    }

    /// Executes all DAG nodes concurrently under an explicit [`EpochGuard`],
    /// returning the completion order of node IDs.
    pub async fn execute_with_guard(self, guard: EpochGuard) -> Result<Vec<String>, DagError> {
        let ids_and_deps: Vec<(String, Vec<String>)> = self
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.deps.clone()))
            .collect();
        let ValidatedTopo {
            index_by_id,
            clean_deps,
            ..
        } = validate_and_toposort(&ids_and_deps)?;

        if guard.is_cancelled() {
            return Err(DagError::Cancelled { epoch: guard.epoch });
        }

        let n = self.nodes.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut tasks_by_idx: Vec<Option<(String, DagTaskFn)>> = self
            .nodes
            .into_iter()
            .map(|node| Some((node.id, node.task)))
            .collect();

        let mut in_degree = vec![0usize; n];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (idx, deps) in clean_deps.iter().enumerate() {
            in_degree[idx] = deps.len();
            for dep_id in deps {
                let dep_idx = index_by_id[dep_id];
                dependents[dep_idx].push(idx);
            }
        }

        let mut join_set: JoinSet<(usize, String, Result<(), String>)> = JoinSet::new();

        // Spawn all initially ready root nodes concurrently.
        for idx in 0..n {
            if in_degree[idx] == 0 {
                if let Some((node_id, task_fn)) = tasks_by_idx[idx].take() {
                    let g = guard.clone();
                    join_set.spawn(async move {
                        let res = task_fn(g).await;
                        (idx, node_id, res)
                    });
                }
            }
        }

        let mut completed_order = Vec::with_capacity(n);

        while !join_set.is_empty() {
            tokio::select! {
                biased;
                () = guard.token.cancelled() => {
                    join_set.abort_all();
                    return Err(DagError::Cancelled { epoch: guard.epoch });
                }
                maybe_joined = join_set.join_next() => {
                    let Some(join_res) = maybe_joined else {
                        break;
                    };
                    match join_res {
                        Ok((done_idx, node_id, Ok(()))) => {
                            if guard.is_cancelled() {
                                join_set.abort_all();
                                return Err(DagError::Cancelled { epoch: guard.epoch });
                            }
                            completed_order.push(node_id);

                            for &dep_idx in &dependents[done_idx] {
                                in_degree[dep_idx] -= 1;
                                if in_degree[dep_idx] == 0 {
                                    if let Some((ready_id, task_fn)) = tasks_by_idx[dep_idx].take() {
                                        let g = guard.clone();
                                        join_set.spawn(async move {
                                            let res = task_fn(g).await;
                                            (dep_idx, ready_id, res)
                                        });
                                    }
                                }
                            }
                        }
                        Ok((_, node_id, Err(error))) => {
                            join_set.abort_all();
                            return Err(DagError::NodeFailed {
                                node: node_id,
                                error,
                            });
                        }
                        Err(join_err) => {
                            join_set.abort_all();
                            if join_err.is_cancelled() && guard.is_cancelled() {
                                return Err(DagError::Cancelled { epoch: guard.epoch });
                            }
                            return Err(DagError::NodePanicked {
                                error: join_err.to_string(),
                            });
                        }
                    }
                }
            }
        }

        if guard.is_cancelled() {
            return Err(DagError::Cancelled { epoch: guard.epoch });
        }

        Ok(completed_order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn dag_plan_computes_waves_exact_critical_path_and_report() {
        let nodes = vec![
            DagNodeSpec::new("resolve_manifest", "Fetch & Parse Version JSON", 60)
                .with_sequential_reason("Asset index & library URLs depend on version manifest"),
            DagNodeSpec::new("fetch_asset_index", "Fetch Asset Index JSON", 50)
                .with_deps(["resolve_manifest"])
                .with_sequential_reason("Object hashes are listed inside asset index"),
            DagNodeSpec::new("download_assets", "Download & SHA1 Assets", 800)
                .with_deps(["fetch_asset_index"])
                .with_parallel_items(3800),
            DagNodeSpec::new("download_libraries", "Download & SHA1 Libraries", 350)
                .with_deps(["resolve_manifest"])
                .with_parallel_items(85),
            DagNodeSpec::new("extract_natives", "Extract LWJGL Natives", 90)
                .with_deps(["download_libraries"])
                .with_parallel_items(8),
            DagNodeSpec::new("download_client", "Download & Verify client.jar", 300)
                .with_deps(["resolve_manifest"])
                .with_parallel_items(1)
                .with_sequential_reason("Single monolithic JAR stream"),
            DagNodeSpec::new("finalize_ready", "Write Installation Lock & Verify", 20)
                .with_deps(["download_assets", "extract_natives", "download_client"])
                .with_sequential_reason("Atomic commit barrier before JVM launch"),
        ];

        let plan = DagPlan::new(nodes).expect("valid acyclic Minecraft install DAG");
        assert_eq!(plan.waves().len(), 4);
        assert_eq!(plan.waves()[0], vec!["resolve_manifest"]);
        assert_eq!(
            plan.waves()[1],
            vec!["fetch_asset_index", "download_libraries", "download_client"]
        );
        assert_eq!(plan.waves()[2], vec!["download_assets", "extract_natives"]);
        assert_eq!(plan.waves()[3], vec!["finalize_ready"]);

        let (crit_path, crit_ms) = plan.critical_path();
        assert_eq!(
            crit_path,
            vec![
                "resolve_manifest",
                "fetch_asset_index",
                "download_assets",
                "finalize_ready"
            ]
        );
        assert_eq!(crit_ms, 60 + 50 + 800 + 20);
        assert_eq!(plan.total_estimated_ms(), 1670);
        assert!(plan.theoretical_parallelism() > 1.79);

        let formatted = plan.format_plan();
        assert!(formatted.contains("Critical Path       : resolve_manifest -> fetch_asset_index -> download_assets -> finalize_ready (930 ms)"));
        assert!(formatted.contains("[CRIT] download_assets"));
        assert!(formatted.contains(
            "reason (sequential): Asset index & library URLs depend on version manifest"
        ));
    }

    #[test]
    fn dag_plan_rejects_cycles_duplicates_and_missing_deps() {
        let dup = vec![
            DagNodeSpec::new("a", "A", 10),
            DagNodeSpec::new("a", "A2", 10),
        ];
        assert_eq!(
            DagPlan::new(dup).unwrap_err(),
            DagError::DuplicateNode("a".to_string())
        );

        let missing = vec![DagNodeSpec::new("a", "A", 10).with_deps(["ghost"])];
        assert_eq!(
            DagPlan::new(missing).unwrap_err(),
            DagError::MissingDependency {
                node: "a".to_string(),
                missing_dep: "ghost".to_string(),
            }
        );

        let cyclic = vec![
            DagNodeSpec::new("a", "A", 10).with_deps(["c"]),
            DagNodeSpec::new("b", "B", 10).with_deps(["a"]),
            DagNodeSpec::new("c", "C", 10).with_deps(["b"]),
        ];
        match DagPlan::new(cyclic).unwrap_err() {
            DagError::CycleDetected(cycle) => {
                assert!(cycle.len() >= 4);
                assert_eq!(cycle.first(), cycle.last());
            }
            other => panic!("expected CycleDetected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn executable_dag_runs_waves_concurrently_and_preserves_dependencies() {
        let mut dag = ExecutableDag::new();
        let active = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        let record_concurrency = |active: Arc<AtomicUsize>, max_c: Arc<AtomicUsize>| async move {
            let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_c.fetch_max(cur, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        };

        dag.add_async_node("root", Vec::<String>::new(), |_g| async move { Ok(()) });

        for branch in ["b1", "b2", "b3"] {
            let a = Arc::clone(&active);
            let m = Arc::clone(&max_concurrent);
            dag.add_async_node(branch, ["root"], move |_g| record_concurrency(a, m));
        }

        dag.add_async_node("sink", ["b1", "b2", "b3"], |_g| async move { Ok(()) });

        let epoch = CancelEpoch::new();
        let start = Instant::now();
        let order = dag.execute(&epoch).await.expect("DAG must succeed");
        let elapsed = start.elapsed();

        assert_eq!(order.first().map(String::as_str), Some("root"));
        assert_eq!(order.last().map(String::as_str), Some("sink"));
        assert_eq!(order.len(), 5);
        // All 3 middle branches must have overlapped concurrently!
        assert_eq!(max_concurrent.load(Ordering::SeqCst), 3);
        assert!(elapsed < Duration::from_millis(2500));
    }

    #[tokio::test]
    async fn executable_dag_cancels_immediately_on_epoch_advance() {
        let mut dag = ExecutableDag::new();
        let sink_ran = Arc::new(AtomicUsize::new(0));
        let sink_ran_clone = Arc::clone(&sink_ran);

        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        dag.add_node(
            "slow_download",
            Vec::<String>::new(),
            Box::new(move |guard| {
                Box::pin(async move {
                    let _ = started_tx.send(());
                    tokio::select! {
                        () = guard.cancelled() => Err("cancelled".to_string()),
                        () = tokio::time::sleep(Duration::from_secs(10)) => Ok(()),
                    }
                })
            }),
        );
        dag.add_async_node("sink", ["slow_download"], move |_g| {
            sink_ran_clone.fetch_add(1, Ordering::SeqCst);
            async move { Ok(()) }
        });

        let epoch = CancelEpoch::new();
        let epoch_runner = epoch.clone();
        let runner = tokio::spawn(async move { dag.execute(&epoch_runner).await });

        started_rx.await.expect("slow_download must start");
        epoch.cancel_and_advance();

        let res = tokio::time::timeout(Duration::from_millis(200), runner)
            .await
            .expect("DAG execution must abort immediately on epoch advance")
            .expect("runner task must not panic");

        assert_eq!(res, Err(DagError::Cancelled { epoch: 1 }));
        assert_eq!(sink_ran.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn executable_dag_propagates_errors_and_skips_dependents() {
        let mut dag = ExecutableDag::new();
        let dependent_ran = Arc::new(AtomicUsize::new(0));
        let dep_clone = Arc::clone(&dependent_ran);

        dag.add_async_node("bad_node", Vec::<String>::new(), |_g| async move {
            Err("SHA1 mismatch on client.jar".to_string())
        });
        dag.add_async_node("downstream", ["bad_node"], move |_g| {
            dep_clone.fetch_add(1, Ordering::SeqCst);
            async move { Ok(()) }
        });

        let epoch = CancelEpoch::new();
        let res = dag.execute(&epoch).await;
        assert_eq!(
            res,
            Err(DagError::NodeFailed {
                node: "bad_node".to_string(),
                error: "SHA1 mismatch on client.jar".to_string(),
            })
        );
        assert_eq!(dependent_ran.load(Ordering::SeqCst), 0);
    }
}
