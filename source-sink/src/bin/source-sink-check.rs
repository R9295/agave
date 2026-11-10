//! Checks source/validate/sink annotations against an LLVM call graph
//! (the text printed by `opt -passes=print-callgraph`, with mangled names).
//!
//! Rule, per identifier: every call path from a source function to a sink
//! function must go through a validate function.
//!
//! The call graph has no ordering and no indirect calls, so this does not
//! prove validation runs *before* the sink, and it cannot see through
//! function pointers, trait objects, channels or threads.

use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead, BufReader},
    process::ExitCode,
};

const KINDS: [&str; 3] = ["source", "validate", "sink"];

#[derive(Default)]
struct Graph {
    names: Vec<String>,
    index: HashMap<String, usize>,
    callees: Vec<Vec<usize>>,
    callers: Vec<Vec<usize>>,
}

impl Graph {
    fn node(&mut self, name: &str) -> usize {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        self.names.push(name.to_string());
        self.callees.push(vec![]);
        self.callers.push(vec![]);
        self.index.insert(name.to_string(), self.names.len() - 1);
        self.names.len() - 1
    }

    fn parse(path: &str) -> Self {
        let mut graph = Self::default();
        let mut reader = BufReader::new(File::open(path).expect("cannot open call graph"));
        let mut current = None;
        let mut buf = vec![];
        while reader.read_until(b'\n', &mut buf).unwrap() > 0 {
            let line = String::from_utf8_lossy(&buf);
            if let Some(rest) = line.strip_prefix("Call graph node for function: '") {
                current = rest.rfind("'<<").map(|end| graph.node(&rest[..end]));
            } else if line.starts_with("Call graph node") {
                current = None;
            } else if let (Some(caller), Some((_, rest))) =
                (current, line.split_once(" calls function '"))
            {
                let callee = graph.node(rest.trim_end().trim_end_matches('\''));
                graph.callees[caller].push(callee);
                graph.callers[callee].push(caller);
            }
            buf.clear();
        }
        graph
    }
}

/// Nodes reachable from `starts` along `edges`, skipping `blocked` nodes.
/// Returns each reached node's predecessor (a start is its own predecessor).
fn reach(starts: &[usize], edges: &[Vec<usize>], blocked: &[bool]) -> Vec<Option<usize>> {
    let mut prev = vec![None; edges.len()];
    let mut stack = vec![];
    for &s in starts.iter().filter(|&&s| !blocked[s]) {
        prev[s] = Some(s);
        stack.push(s);
    }
    while let Some(n) = stack.pop() {
        for &m in &edges[n] {
            if prev[m].is_none() && !blocked[m] {
                prev[m] = Some(n);
                stack.push(m);
            }
        }
    }
    prev
}

fn main() -> ExitCode {
    let path = std::env::args().nth(1).expect("usage: source-sink-check <callgraph>");
    let graph = Graph::parse(&path);
    let unblocked = vec![false; graph.names.len()];

    // id -> [source fns, validate fns, sink fns]; the annotated functions are
    // the callers of the marker functions.
    let mut ids: BTreeMap<String, [Vec<usize>; 3]> = BTreeMap::new();
    for (node, name) in graph.names.iter().enumerate() {
        for (k, kind) in KINDS.iter().enumerate() {
            if let Some((_, id)) = name.rsplit_once(&format!("__{kind}__")) {
                let id = id.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next();
                ids.entry(id.unwrap().to_string()).or_default()[k]
                    .extend(&graph.callers[node]);
            }
        }
    }

    let mut failed = ids.is_empty();
    if failed {
        println!("FAIL: no source/validate/sink markers found in {path}");
    }
    for (id, [sources, validates, sinks]) in &ids {
        println!("[{id}]");
        for (kind, fns) in KINDS.iter().zip([sources, validates, sinks]) {
            for &f in fns {
                println!("  {kind}: {}", graph.names[f]);
            }
        }
        let mut validated = unblocked.clone();
        for &v in validates {
            validated[v] = true;
        }
        let any_path = reach(sources, &graph.callees, &unblocked);
        let bad_path = reach(sources, &graph.callees, &validated);

        if sources.is_empty() || validates.is_empty() || sinks.is_empty() {
            println!("  FAIL: needs at least one source, one validate and one sink");
            failed = true;
        } else if !sinks.iter().any(|&s| any_path[s].is_some()) {
            println!("  FAIL: no sink is reachable from a source in the call graph");
            failed = true;
        } else if let Some(&sink) = sinks.iter().find(|&&s| bad_path[s].is_some()) {
            println!("  FAIL: path from source to sink that avoids validate:");
            let mut path = vec![sink];
            while bad_path[path[path.len() - 1]] != path.last().copied() {
                path.push(bad_path[path[path.len() - 1]].unwrap());
            }
            for &f in path.iter().rev() {
                println!("    -> {}", graph.names[f]);
            }
            failed = true;
        } else {
            println!("  OK: every source -> sink path goes through a validate function");
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
