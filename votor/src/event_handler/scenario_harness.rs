//! Firedancer `test_ag_votor_scenarios` inputs, driven through the real pool and voting loop.
//!
//! This driver starts after signature verification. Certificate signatures are synthetic;
//! cryptography, networking, and asynchronous replay are outside its scope.

#[path = "scenario_driver.rs"]
mod driver;

use {
    crate::{consensus_pool_service::PoolMessage, event::VotorEvent},
    agave_votor_messages::{
        certificate::CertificateType,
        consensus_message::{Block, BlockId},
        vote::Vote,
    },
    driver::Driver,
    serde::Deserialize,
    solana_hash::Hash,
    std::{
        collections::{BTreeMap, BTreeSet, HashSet},
        time::Duration,
    },
};

const MAX_SLOT: u64 = 64;
const MAX_ACTIONS: usize = 256;
const PART_SIGNERS: [[usize; 4]; 3] = [[1, 2, 10, 11], [3, 4, 12, 13], [5, 6, 14, 15]];

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Kind {
    NotarizeCert,
    FinalizeCert,
    NotarFallbackCert,
    FastFinalizeCert,
    SkipCert,
    ReplayArrives,
    ReplayComplete,
    ReplayDead,
    Clock,
    Standstill,
}

#[derive(Deserialize)]
struct RawAction {
    action: Kind,
    node: Option<String>,
    parent: Option<String>,
    ms: Option<u64>,
    part: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Label {
    slot: u64,
    index: u64,
}

impl Label {
    fn parse(text: &str) -> Option<Self> {
        if text == "0" {
            return Some(Self::default());
        }
        let split = text.find(|c: char| !c.is_ascii_digit())?;
        let slot = text[..split].parse::<u64>().ok()?;
        if slot == 0 || slot > MAX_SLOT || text.starts_with('0') {
            return None;
        }
        let mut index = 0u64;
        for byte in text[split..].bytes() {
            if !byte.is_ascii_lowercase() {
                return None;
            }
            index = index
                .checked_mul(26)?
                .checked_add(u64::from(byte - b'a') + 1)?;
        }
        Some(Self {
            slot,
            index: index.checked_sub(1)?,
        })
    }

    fn block(self) -> Block {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&self.slot.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.index.to_le_bytes());
        Block {
            slot: self.slot,
            block_id: BlockId::from(Hash::new_from_array(bytes)),
        }
    }
}

struct Action {
    kind: Kind,
    node: Label,
    parent: Label,
    ms: u64,
    part: Option<usize>,
}

#[derive(Default)]
struct Node {
    parent: Label,
    notar_parts: u8,
    skip_parts: u8,
    finalize: bool,
    fast_finalize: bool,
    fallback: bool,
    replay: bool,
    dead: bool,
}

struct Scenario {
    actions: Vec<Action>,
    nodes: BTreeMap<Label, Node>,
    canonical: BTreeMap<u64, Block>,
    expected_finalized: u64,
}

impl Scenario {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() > 64 * 1024 {
            return None;
        }
        let raw: Vec<RawAction> = serde_json::from_slice(data).ok()?;
        if raw.len() > MAX_ACTIONS {
            return None;
        }
        let mut actions = Vec::with_capacity(raw.len());
        let mut nodes = BTreeMap::<Label, Node>::new();
        let mut replayed = BTreeSet::from([Label::default()]);
        for raw in raw {
            let (node, parent) = match raw.action {
                Kind::Clock | Kind::Standstill => {
                    if raw.node.is_some()
                        || raw.parent.is_some()
                        || raw.part.is_some()
                        || (raw.action == Kind::Standstill && raw.ms.is_some())
                    {
                        return None;
                    }
                    (Label::default(), Label::default())
                }
                _ => {
                    if raw.ms.is_some() {
                        return None;
                    }
                    let node = Label::parse(raw.node.as_deref()?)?;
                    let parent = Label::parse(raw.parent.as_deref()?)?;
                    if parent.slot >= node.slot {
                        return None;
                    }
                    if let Some(part) = raw.part {
                        if part >= 3 || !matches!(raw.action, Kind::NotarizeCert | Kind::SkipCert) {
                            return None;
                        }
                    }
                    let entry = nodes.entry(node).or_insert_with(|| Node {
                        parent,
                        ..Node::default()
                    });
                    if entry.parent != parent {
                        return None;
                    }
                    let parts = raw.part.map_or(7, |part| 1 << part);
                    match raw.action {
                        Kind::NotarizeCert => entry.notar_parts |= parts,
                        Kind::SkipCert => entry.skip_parts |= parts,
                        Kind::FinalizeCert => entry.finalize = true,
                        Kind::FastFinalizeCert => entry.fast_finalize = true,
                        Kind::NotarFallbackCert => entry.fallback = true,
                        Kind::ReplayComplete => {
                            if !replayed.contains(&parent) {
                                return None;
                            }
                            replayed.insert(node);
                            entry.replay = true;
                        }
                        Kind::ReplayDead => entry.dead = true,
                        _ => (),
                    }
                    (node, parent)
                }
            };
            let ms = if raw.action == Kind::Clock {
                raw.ms?
            } else {
                0
            };
            // Bound virtual time as well as allocations before constructing any banks.
            if ms > 1_000_000 {
                return None;
            }
            actions.push(Action {
                kind: raw.action,
                node,
                parent,
                ms,
                part: raw.part,
            });
        }
        for node in nodes.values() {
            if (node.dead && node.replay)
                || (node.parent.slot != 0 && !nodes.contains_key(&node.parent))
            {
                return None;
            }
        }
        // The reference chooses the deepest, leftmost non-skipped leaf.
        let leaf = nodes
            .iter()
            .filter(|(_, node)| node.skip_parts == 0)
            .max_by(|(a, _), (b, _)| a.slot.cmp(&b.slot).then_with(|| b.index.cmp(&a.index)))
            .map(|(&label, _)| label);
        let mut canonical = BTreeMap::from([(0, Block::default())]);
        let mut current = leaf;
        while let Some(label) = current {
            canonical.insert(label.slot, label.block());
            let parent = nodes.get(&label)?.parent;
            current = (parent.slot != 0).then_some(parent);
        }
        let mut certified = BTreeMap::<u64, BTreeSet<Block>>::new();
        let mut expected_finalized = 0;
        for (&label, node) in &nodes {
            let is_canonical = canonical.get(&label.slot) == Some(&label.block());
            // Contradictory quorum certificates violate the scenario oracle's assumptions.
            if (!is_canonical && (node.notar_parts == 7 || node.finalize || node.fast_finalize))
                || (canonical.contains_key(&label.slot) && node.skip_parts != 0)
            {
                return None;
            }
            if node.fallback || node.notar_parts == 7 || node.fast_finalize {
                let blocks = certified.entry(label.slot).or_default();
                blocks.insert(label.block());
                if blocks.len() > 4 {
                    return None;
                }
            }
            if node.fast_finalize || (node.notar_parts == 7 && node.finalize) {
                expected_finalized = expected_finalized.max(label.slot);
            }
        }
        Some(Self {
            actions,
            nodes,
            canonical,
            expected_finalized,
        })
    }

    fn check(&self, driver: &mut Driver, seen: &mut HashSet<Vote>, root: &mut u64) {
        for (vote, refresh) in driver.take_votes() {
            if refresh {
                assert!(
                    seen.contains(&vote),
                    "standstill refreshed an unseen vote: {vote:?}"
                );
                continue;
            }
            match vote {
                Vote::Notarize(notar) => {
                    assert!(
                        driver.has_replayed(notar.block),
                        "notarized an unreplayed block"
                    );
                    assert!(
                        !seen.contains(&Vote::new_skip_vote(notar.block.slot)),
                        "notarized a skipped slot"
                    );
                    assert!(
                        !seen
                            .iter()
                            .any(|v| v.is_notarization() && v.slot() == notar.block.slot),
                        "notarized twice in one slot"
                    );
                }
                Vote::Skip(skip) => {
                    assert!(!seen.contains(&vote), "skipped twice in one slot");
                    assert!(
                        !seen
                            .iter()
                            .any(|v| v.is_notarization() && v.slot() == skip.slot),
                        "skipped a notarized slot"
                    );
                }
                Vote::Finalize(finalize) => {
                    let block = self
                        .canonical
                        .get(&finalize.slot)
                        .expect("final vote off canonical chain");
                    assert!(
                        seen.contains(&Vote::new_notarization_vote(*block)),
                        "final vote without own canonical notar vote"
                    );
                }
                _ => (),
            }
            seen.insert(vote);
        }
        assert!(driver.vote_root() >= *root, "root regressed");
        *root = driver.vote_root();
        if let Some(block) = driver.rooted_block() {
            assert_eq!(
                self.canonical.get(&block.slot),
                Some(&block),
                "root off canonical chain"
            );
            let certs = driver.certificates();
            assert!(
                certs.contains(&CertificateType::FinalizeFast(block))
                    || (certs.contains(&CertificateType::Notarize(block))
                        && certs.contains(&CertificateType::Finalize(block.slot))),
                "root without supporting certificates"
            );
            // Replay ancestry is the support for implicit ancestor finalization.
            let mut ancestor = block;
            while ancestor.slot != 0 {
                assert!(
                    driver.has_replayed(ancestor),
                    "root has an unreplayed ancestor"
                );
                assert_eq!(self.canonical.get(&ancestor.slot), Some(&ancestor));
                let label = self
                    .nodes
                    .keys()
                    .find(|label| label.block() == ancestor)
                    .unwrap();
                ancestor = self.nodes[label].parent.block();
            }
        }
        assert!(
            driver.highest_finalized() <= self.expected_finalized,
            "unexpected finalization"
        );
    }
}

/// Run a bounded Firedancer JSON scenario, panicking on an invariant violation.
/// Returns false for malformed or inconsistent inputs, before constructing the driver.
pub fn run(data: &[u8]) -> bool {
    let Some(scenario) = Scenario::parse(data) else {
        return false;
    };
    let mut driver = Driver::new();
    let mut seen = HashSet::from([Vote::new_notarization_vote(Block::default())]);
    // The signature verifier supplies nonoverlapping external aggregates to the pool.
    let mut delivered = HashSet::new();
    let mut root = 0;
    for action in &scenario.actions {
        let block = action.node.block();
        match action.kind {
            Kind::NotarizeCert | Kind::SkipCert => {
                let vote = if action.kind == Kind::NotarizeCert {
                    Vote::new_notarization_vote(block)
                } else {
                    Vote::new_skip_vote(block.slot)
                };
                let parts: &[usize] = match action.part {
                    Some(ref part) => std::slice::from_ref(part),
                    None => &[0, 1, 2],
                };
                for &part in parts {
                    let votes = PART_SIGNERS[part]
                        .iter()
                        .filter(|&&index| delivered.insert((vote, index)))
                        .map(|&index| driver.external_vote(index, vote))
                        .collect();
                    driver.pool_message(PoolMessage::Votes(votes));
                    scenario.check(&mut driver, &mut seen, &mut root);
                }
            }
            Kind::FinalizeCert | Kind::FastFinalizeCert | Kind::NotarFallbackCert => {
                let cert_type = match action.kind {
                    Kind::FinalizeCert => CertificateType::Finalize(block.slot),
                    Kind::FastFinalizeCert => CertificateType::FinalizeFast(block),
                    _ => CertificateType::NotarizeFallback(block),
                };
                let cert = driver.certificate(cert_type);
                driver.pool_message(PoolMessage::Certificates(vec![cert]));
            }
            Kind::ReplayComplete => {
                driver.replay(block, action.parent.block());
            }
            Kind::ReplayDead => driver.dead(block),
            Kind::ReplayArrives => (),
            Kind::Clock => driver.advance_clock(Duration::from_millis(action.ms)),
            Kind::Standstill => driver.event(VotorEvent::Standstill(driver.highest_finalized())),
        }
        scenario.check(&mut driver, &mut seen, &mut root);
    }
    driver.advance_clock(Duration::from_secs(1000));
    scenario.check(&mut driver, &mut seen, &mut root);
    assert_eq!(
        driver.highest_finalized(),
        scenario.expected_finalized,
        "missing finalization"
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_scenarios() {
        for scenario in [
            include_bytes!("../../fuzz/seeds/slow.json").as_slice(),
            include_bytes!("../../fuzz/seeds/fast.json").as_slice(),
            include_bytes!("../../fuzz/seeds/skip.json").as_slice(),
            include_bytes!("../../fuzz/seeds/fallback.json").as_slice(),
            include_bytes!("../../fuzz/seeds/fork.json").as_slice(),
        ] {
            assert!(run(scenario));
        }
    }

    #[test]
    fn duplicate_parts_do_not_create_a_quorum() {
        assert!(run(br#"[
            {"node":"1a","parent":"0","action":"REPLAY_COMPLETE"},
            {"node":"1a","parent":"0","action":"NOTARIZE_CERT","part":0},
            {"node":"1a","parent":"0","action":"NOTARIZE_CERT","part":0},
            {"node":"1a","parent":"0","action":"NOTARIZE_CERT","part":0},
            {"node":"1a","parent":"0","action":"FINALIZE_CERT"}
        ]"#));
    }

    #[test]
    fn reject_invalid_scenarios() {
        for input in [
            "not json",
            r#"[{"action":"CLOCK"}]"#,
            r#"[{"node":"1a","parent":"1a","action":"REPLAY_COMPLETE"}]"#,
            r#"[{"node":"2a","parent":"1a","action":"REPLAY_COMPLETE"}]"#,
            r#"[{"node":"1a","parent":"0","action":"NOTARIZE_CERT","part":3}]"#,
            r#"[{"node":"999999999999a","parent":"0","action":"FAST_FINALIZE_CERT"}]"#,
            r#"[{"node":"1a","parent":"0","action":"FAST_FINALIZE_CERT"},{"node":"1b","parent":"0","action":"NOTARIZE_CERT"}]"#,
            r#"[{"node":"1a","parent":"0","action":"NOTARIZE_CERT"},{"node":"1b","parent":"0","action":"SKIP_CERT"}]"#,
        ] {
            assert!(!run(input.as_bytes()), "accepted {input}");
        }
    }
}
