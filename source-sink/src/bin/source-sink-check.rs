//! Checks source/validate/sink annotations against an LLVM call graph
//! (the text printed by `opt -passes=print-callgraph`, with mangled names).
//!
//! Two rules, per identifier:
//!
//! - Reverse (always): walk up the callers of each sink. Every chain must hit
//!   a validate function before it ends (a function with no callers) or gets
//!   `--max-depth` (default 100) calls away from the sink. At most
//!   `--max-callers` (default 50) callers of each function are followed.
//! - Forward (only if the identifier has a source): every call path from a
//!   source function to a sink function must go through a validate function.
//!
//! `--known-issues FILE` lists call chains that are known false positives.
//! Chains are separated by blank lines, one demangled function name per line,
//! in either direction; `#` starts a comment and a leading `<- ` or `-> ` is
//! ignored, so chains can be pasted from this tool's output. A chain that
//! contains a known chain as a consecutive run of calls is not reported.
//!
//! The call graph has no ordering and no indirect calls, so this does not
//! prove validation runs *before* the sink, and it cannot see through
//! function pointers, trait objects, channels or threads.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
    process::ExitCode,
};

const KINDS: [&str; 3] = ["source", "validate", "sink"];

#[derive(Default)]
struct Graph {
    names: Vec<String>,
    /// Demangled names, used for output and for matching known issues.
    pretty: Vec<String>,
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
        self.pretty.push(format!("{:#}", rustc_demangle::demangle(name)));
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
        for edges in graph.callees.iter_mut().chain(&mut graph.callers) {
            edges.sort_unstable();
            edges.dedup();
        }
        graph
    }
}

/// Reads known-issue chains, and adds each one reversed as well so that it
/// matches both the reverse and the forward walk.
fn parse_known_issues(path: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).expect("cannot open known issues file");
    let mut chains = vec![vec![]];
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let line = line.trim_start_matches("<- ").trim_start_matches("-> ");
        if line.is_empty() {
            chains.push(vec![]);
        } else {
            chains.last_mut().unwrap().push(line.to_string());
        }
    }
    chains.retain(|chain| !chain.is_empty());
    let reversed: Vec<_> = chains.iter().map(|c| c.iter().rev().cloned().collect()).collect();
    chains.extend(reversed);
    chains
}

#[derive(Default)]
struct Outcome {
    /// A failing chain, in walk order, and why it fails.
    bad: Option<(Vec<usize>, &'static str)>,
    /// Edges not followed because of `max_fanout`.
    skipped: usize,
    /// Chains dropped because they contain a known issue.
    ignored: usize,
}

/// Walks `edges` breadth-first from `starts`, not going past `blocked`
/// functions, and returns the first chain for which `fail(node, number of
/// edges, depth)` gives a reason.
///
/// A walk state is a function plus how much of each known chain the calls
/// leading to it have matched, so a chain that contains a known issue is
/// dropped without hiding other chains through the same functions.
fn walk(
    graph: &Graph,
    starts: &[usize],
    edges: &[Vec<usize>],
    blocked: &[bool],
    known: &[Vec<String>],
    max_fanout: usize,
    fail: impl Fn(usize, usize, usize) -> Option<&'static str>,
) -> Outcome {
    // Advances the known-chain matches by one function; None if one completes.
    let step = |progress: &[usize], node: usize| -> Option<Vec<usize>> {
        let name = &graph.pretty[node];
        let mut next = vec![];
        for (chain, &matched) in known.iter().zip(progress) {
            let matched = if chain[matched] == *name {
                matched + 1
            } else {
                usize::from(chain[0] == *name)
            };
            if matched == chain.len() {
                return None;
            }
            next.push(matched);
        }
        Some(next)
    };

    let mut outcome = Outcome::default();
    // (function, known-chain progress, index of the previous state)
    let mut states: Vec<(usize, Vec<usize>, usize)> = vec![];
    let mut seen = HashSet::new();
    let mut level = vec![];
    for &start in starts {
        match step(&vec![0; known.len()], start) {
            Some(progress) if seen.insert((start, progress.clone())) => {
                level.push(states.len());
                states.push((start, progress, states.len()));
            }
            Some(_) => {}
            None => outcome.ignored += 1,
        }
    }
    for depth in 0.. {
        let mut next_level = vec![];
        for &state in &level {
            let (node, progress, _) = states[state].clone();
            if blocked[node] {
                continue;
            }
            if let Some(why) = fail(node, edges[node].len(), depth) {
                let mut chain = vec![];
                let mut at = state;
                loop {
                    chain.push(states[at].0);
                    if states[at].2 == at {
                        break;
                    }
                    at = states[at].2;
                }
                chain.reverse();
                outcome.bad = Some((chain, why));
                return outcome;
            }
            outcome.skipped += edges[node].len().saturating_sub(max_fanout);
            for &next in edges[node].iter().take(max_fanout) {
                match step(&progress, next) {
                    Some(progress) if seen.insert((next, progress.clone())) => {
                        next_level.push(states.len());
                        states.push((next, progress, state));
                    }
                    Some(_) => {}
                    None => outcome.ignored += 1,
                }
            }
        }
        if next_level.is_empty() {
            break;
        }
        level = next_level;
    }
    outcome
}

fn report(graph: &Graph, direction: &str, arrow: &str, ok: &str, outcome: Outcome) -> bool {
    if outcome.skipped > 0 {
        println!("  note ({direction}): {} caller(s) not followed", outcome.skipped);
    }
    if outcome.ignored > 0 {
        println!("  note ({direction}): {} chain(s) ignored as known issues", outcome.ignored);
    }
    match outcome.bad {
        Some((chain, why)) => {
            println!("  FAIL ({direction}): {why}:");
            for &f in &chain {
                println!("    {arrow} {}", graph.pretty[f]);
            }
            true
        }
        None => {
            println!("  OK ({direction}): {ok}");
            false
        }
    }
}

fn main() -> ExitCode {
    let (mut path, mut known, mut max_callers, mut max_depth) = (None, vec![], 50, 100);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().expect("option needs a value");
        match arg.as_str() {
            "--max-callers" => max_callers = value().parse().expect("expected a number"),
            "--max-depth" => max_depth = value().parse().expect("expected a number"),
            "--known-issues" => known = parse_known_issues(&value()),
            _ => path = Some(arg),
        }
    }
    let path = path.expect(
        "usage: source-sink-check [--max-callers N] [--max-depth N] [--known-issues FILE] \
         <callgraph>",
    );
    let graph = Graph::parse(&path);

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
                println!("  {kind}: {}", graph.pretty[f]);
            }
        }
        if validates.is_empty() || sinks.is_empty() {
            println!("  FAIL: needs at least one validate and one sink");
            failed = true;
            continue;
        }
        let mut is_validate = vec![false; graph.names.len()];
        let mut is_sink = is_validate.clone();
        validates.iter().for_each(|&v| is_validate[v] = true);
        sinks.iter().for_each(|&s| is_sink[s] = true);

        let reverse = walk(
            &graph,
            sinks,
            &graph.callers,
            &is_validate,
            &known,
            max_callers,
            |_, callers, depth| match (callers, depth == max_depth) {
                (0, _) => Some("chain ends without a validate function above the sink"),
                (_, true) => Some("depth limit reached without a validate function"),
                _ => None,
            },
        );
        failed |= report(
            &graph,
            "reverse",
            "<-",
            "every caller chain of the sink hits a validate function",
            reverse,
        );

        if sources.is_empty() {
            continue;
        }
        let forward = walk(
            &graph,
            sources,
            &graph.callees,
            &is_validate,
            &known,
            usize::MAX,
            |node, _, _| is_sink[node].then_some("path from source to sink that avoids validate"),
        );
        failed |= report(
            &graph,
            "forward",
            "->",
            "every source -> sink path goes through a validate function",
            forward,
        );
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
