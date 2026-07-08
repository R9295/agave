//! Structural mutations over a parsed IL [`Program`].
//!
//! A mutation targets the account-meta list of a call ([`Statement::Invoke`]).
//! Each account meta carries `is_writable` and `is_signer` flags (mirroring the
//! `(pubkey, is_writable, is_signer)` tuples in the IL source); flipping one of
//! these is a cheap way to explore signer/writable privilege escalation and
//! under-privilege paths through the program.

// The mutator is a standalone primitive not yet wired into a fuzz harness.
#![allow(dead_code)]

use crate::{
    il::{Program, Statement},
    lower::LoweredProgram,
};

/// Identifies a single account meta inside a call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountMetaSite {
    /// Index into [`Program::statements`] of the enclosing `Invoke`.
    pub statement: usize,
    /// Index into that call's account-meta list.
    pub account: usize,
}

/// The account-meta flag a mutation targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flag {
    Signer,
    Writable,
}

/// Enumerate every account meta across every call in `program`.
///
/// Sites are returned in program order, so a stable `seed` maps to a stable
/// site for a given program.
pub fn account_meta_sites(program: &Program) -> Vec<AccountMetaSite> {
    let mut sites = Vec::new();
    for (statement, stmt) in program.statements.iter().enumerate() {
        if let Statement::Invoke {
            accounts: Some(accounts),
            ..
        } = stmt
        {
            for account in 0..accounts.len() {
                sites.push(AccountMetaSite { statement, account });
            }
        }
    }
    sites
}

/// Flip `flag` at `site`. Returns the new flag value, or `None` if the site
/// does not resolve to an account meta in `program`.
pub fn flip(program: &mut Program, site: AccountMetaSite, flag: Flag) -> Option<bool> {
    let Statement::Invoke {
        accounts: Some(accounts),
        ..
    } = program.statements.get_mut(site.statement)?
    else {
        return None;
    };
    let meta = accounts.get_mut(site.account)?;
    let target = match flag {
        Flag::Signer => &mut meta.is_signer,
        Flag::Writable => &mut meta.is_writable,
    };
    *target = !*target;
    Some(*target)
}

/// Flip `is_signer` on the account meta selected by `seed` (modulo the number
/// of account metas in the program). Returns the mutated [`AccountMetaSite`],
/// or `None` when the program has no account metas to mutate.
pub fn flip_is_signer(program: &mut Program, seed: usize) -> Option<AccountMetaSite> {
    flip_selected(program, seed, Flag::Signer)
}

/// Flip `is_writable` on the account meta selected by `seed` (modulo the number
/// of account metas in the program). Returns the mutated [`AccountMetaSite`],
/// or `None` when the program has no account metas to mutate.
pub fn flip_is_writable(program: &mut Program, seed: usize) -> Option<AccountMetaSite> {
    flip_selected(program, seed, Flag::Writable)
}

fn flip_selected(program: &mut Program, seed: usize, flag: Flag) -> Option<AccountMetaSite> {
    let sites = account_meta_sites(program);
    if sites.is_empty() {
        return None;
    }
    let site = sites[seed % sites.len()];
    flip(program, site, flag).map(|_| site)
}

// ---------------------------------------------------------------------------
// Lowered-program mutations.
//
// The IL `Program` above is the parse-stage view. Once lowered, each call
// becomes an `Invocation`; the system-program CPIs (CreateAccount, Transfer,
// Assign, the nonce ops, ...) are `InvocationKind::System`. The account metas
// that a `System` invocation passes to `sol_invoke_signed_c` carry the same
// `is_signer`/`is_writable` flags, so the same flips apply here — this is where
// they bite for the actual system calls.
// ---------------------------------------------------------------------------

/// Identifies a single account meta inside a lowered invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvocationMetaSite {
    /// Index into [`LoweredProgram::invocations`].
    pub invocation: usize,
    /// Index into that invocation's meta list.
    pub account: usize,
}

/// Enumerate account-meta sites across every one of the lowered program's
/// invocations, of any kind (system-program CPIs and direct-manipulation calls
/// alike).
pub fn invocation_meta_sites(program: &LoweredProgram) -> Vec<InvocationMetaSite> {
    let mut sites = Vec::new();
    for (invocation, inv) in program.invocations.iter().enumerate() {
        for account in 0..inv.metas.len() {
            sites.push(InvocationMetaSite {
                invocation,
                account,
            });
        }
    }
    sites
}

/// Flip `flag` at `site` in the lowered program. Returns the new flag value, or
/// `None` if the site does not resolve to a meta.
pub fn flip_lowered(
    program: &mut LoweredProgram,
    site: InvocationMetaSite,
    flag: Flag,
) -> Option<bool> {
    let meta = program
        .invocations
        .get_mut(site.invocation)?
        .metas
        .get_mut(site.account)?;
    let target = match flag {
        Flag::Signer => &mut meta.is_signer,
        Flag::Writable => &mut meta.is_writable,
    };
    *target = !*target;
    Some(*target)
}

/// Flip `is_signer` on the invocation meta selected by `seed` (any invocation
/// kind is eligible).
pub fn flip_invocation_is_signer(
    program: &mut LoweredProgram,
    seed: usize,
) -> Option<InvocationMetaSite> {
    flip_lowered_selected(program, seed, Flag::Signer)
}

/// Flip `is_writable` on the invocation meta selected by `seed` (any invocation
/// kind is eligible).
pub fn flip_invocation_is_writable(
    program: &mut LoweredProgram,
    seed: usize,
) -> Option<InvocationMetaSite> {
    flip_lowered_selected(program, seed, Flag::Writable)
}

fn flip_lowered_selected(
    program: &mut LoweredProgram,
    seed: usize,
    flag: Flag,
) -> Option<InvocationMetaSite> {
    let sites = invocation_meta_sites(program);
    if sites.is_empty() {
        return None;
    }
    let site = sites[seed % sites.len()];
    flip_lowered(program, site, flag).map(|_| site)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{il::parse_program, lower::lower_il},
    };

    const SOURCE: &str = r"LoadU64 lamports = 50
CreateAccount | lamports ;
  (account:1, true, false),
  (account:2, false, true)";

    fn metas(program: &Program) -> Vec<(bool, bool)> {
        program
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                Statement::Invoke {
                    accounts: Some(accounts),
                    ..
                } => Some(accounts),
                _ => None,
            })
            .flatten()
            .map(|meta| (meta.is_writable, meta.is_signer))
            .collect()
    }

    #[test]
    fn enumerates_every_meta() {
        let program = parse_program(SOURCE).unwrap();
        assert_eq!(
            account_meta_sites(&program),
            vec![
                AccountMetaSite {
                    statement: 1,
                    account: 0
                },
                AccountMetaSite {
                    statement: 1,
                    account: 1
                },
            ]
        );
    }

    #[test]
    fn flip_is_signer_toggles_only_signer() {
        let mut program = parse_program(SOURCE).unwrap();
        let site = flip_is_signer(&mut program, 0).unwrap();
        assert_eq!(site.account, 0);
        // is_writable untouched, is_signer flipped false -> true.
        assert_eq!(metas(&program), vec![(true, true), (false, true)]);
    }

    #[test]
    fn flip_is_writable_toggles_only_writable() {
        let mut program = parse_program(SOURCE).unwrap();
        let site = flip_is_writable(&mut program, 1).unwrap();
        assert_eq!(site.account, 1);
        // second meta: is_writable false -> true, is_signer untouched.
        assert_eq!(metas(&program), vec![(true, false), (true, true)]);
    }

    #[test]
    fn flip_is_idempotent_in_pairs() {
        let mut program = parse_program(SOURCE).unwrap();
        let before = metas(&program);
        let site = flip_is_signer(&mut program, 0).unwrap();
        flip(&mut program, site, Flag::Signer);
        assert_eq!(metas(&program), before);
    }

    #[test]
    fn no_metas_returns_none() {
        let mut program = parse_program("LoadU64 lamports = 50").unwrap();
        assert_eq!(flip_is_signer(&mut program, 0), None);
        assert_eq!(flip_is_writable(&mut program, 0), None);
    }

    // A system-program CPI (Transfer) followed by a direct-manipulation call
    // (WriteAccountData). Only the former is `InvocationKind::System`.
    const LOWERED_SOURCE: &str = r"LoadU64 amount = 10
Transfer | amount ;
  (account:1, true, true),
  (account:2, true, false)
WriteAccountData | 0, 1, 2 ;
  (account:3, true, false)";

    fn lowered_metas(program: &LoweredProgram) -> Vec<Vec<(bool, bool)>> {
        program
            .invocations
            .iter()
            .map(|inv| {
                inv.metas
                    .iter()
                    .map(|m| (m.is_writable, m.is_signer))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn lowered_sites_cover_all_invocations() {
        let program = lower_il(LOWERED_SOURCE).unwrap();
        // Transfer (system CPI) contributes 2 metas, WriteAccountData (direct) 1.
        let sites = invocation_meta_sites(&program);
        assert_eq!(sites.len(), 3);
        // Sites span both invocation kinds, not just the system CPI.
        assert!(sites.iter().any(|s| s.invocation == 0));
        assert!(sites.iter().any(|s| s.invocation == 1));
    }

    #[test]
    fn flip_invocation_flags_toggle_in_isolation() {
        let mut program = lower_il(LOWERED_SOURCE).unwrap();
        let site = flip_invocation_is_signer(&mut program, 1).unwrap();
        assert_eq!(
            site,
            InvocationMetaSite {
                invocation: 0,
                account: 1
            }
        );
        // Transfer account:2 is_signer false -> true; everything else untouched.
        assert_eq!(
            lowered_metas(&program),
            vec![vec![(true, true), (true, true)], vec![(true, false)]]
        );
    }

    #[test]
    fn flip_can_land_on_a_direct_call_meta() {
        let mut program = lower_il(LOWERED_SOURCE).unwrap();
        // seed 2 selects the third site — the WriteAccountData meta.
        let site = flip_invocation_is_writable(&mut program, 2).unwrap();
        assert_eq!(site.invocation, 1);
        assert_eq!(
            lowered_metas(&program),
            vec![vec![(true, true), (true, false)], vec![(false, false)]]
        );
    }

    #[test]
    fn no_invocation_metas_returns_none() {
        let mut program = lower_il("LoadU64 x = 1").unwrap();
        assert_eq!(flip_invocation_is_signer(&mut program, 0), None);
        assert_eq!(flip_invocation_is_writable(&mut program, 0), None);
    }
}
