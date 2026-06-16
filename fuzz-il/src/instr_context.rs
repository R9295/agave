use {
    crate::{
        compiler,
        il::{self, AccountState, AccountStateTarget, AddressExpr, IlError},
        lower::{Invocation, LoweredAccountState, LoweredProgram, MetaPubkey},
    },
    prost::Message,
    protosol::protos::{AcctState, InstrAcct, InstrContext},
    solana_nonce::{
        state::{Data as NonceData, DurableNonce, State as NonceState},
        versions::Versions as NonceVersions,
    },
    solana_pubkey::Pubkey,
    solana_sdk_ids::{native_loader, system_program, sysvar},
    std::{collections::HashMap, path::PathBuf},
};

pub(crate) fn print_lowered(program: &LoweredProgram) -> il::Result<()> {
    for (index, context) in lowered_to_instr_contexts(program)?.iter().enumerate() {
        print_instr_context(index, context);
        let path = write_instr_context(index, context)?;
        eprintln!("InstrContext[{index}] protobuf: {}", display_path(&path));
    }
    Ok(())
}

pub(crate) fn lowered_to_instr_contexts(program: &LoweredProgram) -> il::Result<Vec<InstrContext>> {
    let account_states = AccountStateOverrides::from_lowered(&program.account_states);
    program
        .invocations
        .iter()
        .map(|invocation| invocation_to_instr_context(invocation, &account_states))
        .collect()
}

fn invocation_to_instr_context(
    invocation: &Invocation,
    account_states: &AccountStateOverrides,
) -> il::Result<InstrContext> {
    let mut accounts = ContextAccounts::new(account_states);
    let mut instr_accounts = Vec::with_capacity(invocation.metas.len());

    for meta in &invocation.metas {
        let account_index = accounts.index_for_meta(&meta.pubkey)?;
        instr_accounts.push(InstrAcct {
            index: account_index,
            is_writable: meta.is_writable,
            is_signer: meta.is_signer,
        });
    }

    accounts.index_for_address(system_program::id().to_bytes())?;
    accounts.index_for_address(sysvar::clock::id().to_bytes())?;
    accounts.index_for_address(sysvar::rent::id().to_bytes())?;

    Ok(InstrContext {
        program_id: system_program::id().to_bytes().to_vec(),
        accounts: accounts.into_accounts(),
        instr_accounts,
        data: patched_instruction_data(invocation)?,
        cu_avail: 1_400_000,
        features: None,
    })
}

#[derive(Default)]
struct AccountStateOverrides {
    by_account: HashMap<usize, AccountState>,
    by_address: HashMap<[u8; 32], AccountState>,
}

impl AccountStateOverrides {
    fn from_lowered(states: &[LoweredAccountState]) -> Self {
        let mut overrides = Self::default();
        for state in states {
            match &state.target {
                AccountStateTarget::Account(index) => {
                    overrides.by_account.insert(*index, state.state.clone());
                }
                AccountStateTarget::Address(address) => {
                    overrides
                        .by_address
                        .insert(address_expr_bytes(address), state.state.clone());
                }
            }
        }
        overrides
    }

    fn for_meta(&self, pubkey: &MetaPubkey, address: &[u8; 32]) -> Option<&AccountState> {
        match pubkey {
            MetaPubkey::Account(index) => self
                .by_account
                .get(index)
                .or_else(|| self.by_address.get(address)),
            MetaPubkey::ProgramId | MetaPubkey::Known(_) | MetaPubkey::Literal(_) => {
                self.by_address.get(address)
            }
        }
    }

    fn for_address(&self, address: &[u8; 32]) -> Option<&AccountState> {
        self.by_address.get(address)
    }
}

struct ContextAccounts<'a> {
    accounts: Vec<AcctState>,
    by_address: HashMap<[u8; 32], u32>,
    overrides: &'a AccountStateOverrides,
}

impl<'a> ContextAccounts<'a> {
    fn new(overrides: &'a AccountStateOverrides) -> Self {
        Self {
            accounts: Vec::new(),
            by_address: HashMap::new(),
            overrides,
        }
    }

    fn index_for_meta(&mut self, pubkey: &MetaPubkey) -> il::Result<u32> {
        let address = meta_pubkey_bytes(pubkey)?;
        self.index_for(address, self.overrides.for_meta(pubkey, &address))
    }

    fn index_for_address(&mut self, address: [u8; 32]) -> il::Result<u32> {
        self.index_for(address, self.overrides.for_address(&address))
    }

    fn index_for(
        &mut self,
        address: [u8; 32],
        override_state: Option<&AccountState>,
    ) -> il::Result<u32> {
        if let Some(index) = self.by_address.get(&address) {
            return Ok(*index);
        }
        let index = u32::try_from(self.accounts.len())
            .map_err(|_| IlError::new("too many InstrContext accounts"))?;
        self.accounts.push(account_state(address, override_state)?);
        self.by_address.insert(address, index);
        Ok(index)
    }

    fn into_accounts(self) -> Vec<AcctState> {
        self.accounts
    }
}

fn patched_instruction_data(invocation: &Invocation) -> il::Result<Vec<u8>> {
    let mut data = invocation.data.clone();
    for patch in &invocation.patches {
        let end = patch
            .offset
            .checked_add(32)
            .ok_or_else(|| IlError::new("instruction patch offset overflow"))?;
        if end > data.len() {
            return Err(IlError::new(format!(
                "instruction patch range {}..{end} exceeds {} bytes",
                patch.offset,
                data.len()
            )));
        }
        data[patch.offset..end].copy_from_slice(&address_expr_bytes(&patch.source));
    }
    Ok(data)
}

fn meta_pubkey_bytes(pubkey: &MetaPubkey) -> il::Result<[u8; 32]> {
    match pubkey {
        MetaPubkey::Account(index) => Ok(synthetic_account_key(*index)),
        MetaPubkey::ProgramId => Ok(system_program::id().to_bytes()),
        MetaPubkey::Known("SYSVAR_RENT_ID") => Ok(sysvar::rent::id().to_bytes()),
        MetaPubkey::Known("SYSVAR_RECENT_BLOCKHASHES_ID") => {
            Ok(sysvar::recent_blockhashes::id().to_bytes())
        }
        MetaPubkey::Known(name) => Err(IlError::new(format!("unknown account meta `{name}`"))),
        MetaPubkey::Literal(pubkey) => Ok(pubkey.0),
    }
}

fn address_expr_bytes(address: &AddressExpr) -> [u8; 32] {
    match address {
        AddressExpr::Static(pubkey) => pubkey.0,
        AddressExpr::AccountKey(index) => synthetic_account_key(*index),
        AddressExpr::ProgramId => system_program::id().to_bytes(),
    }
}

fn synthetic_account_key(index: usize) -> [u8; 32] {
    let mut key = [0; 32];
    key[..15].copy_from_slice(b"fuzz-il-account");
    key[24..].copy_from_slice(&(index as u64).to_le_bytes());
    key
}

fn account_state(
    address: [u8; 32],
    override_state: Option<&AccountState>,
) -> il::Result<AcctState> {
    if let Some(state) = override_state {
        return account_state_from_decl(address, state);
    }

    let system_program = system_program::id().to_bytes();
    if address == system_program {
        return system_program_account(address);
    }
    if address == sysvar::clock::id().to_bytes() {
        return clock_sysvar_account(address);
    }
    if address == sysvar::rent::id().to_bytes() {
        return rent_sysvar_account(address);
    }
    if address == sysvar::recent_blockhashes::id().to_bytes() {
        return recent_blockhashes_sysvar_account(address);
    }
    Ok(acct_state(
        address,
        system_program,
        1_000_000_000,
        Vec::new(),
        false,
    ))
}

fn account_state_from_decl(address: [u8; 32], state: &AccountState) -> il::Result<AcctState> {
    match state {
        AccountState::SystemFunded { lamports } => Ok(acct_state(
            address,
            system_program::id().to_bytes(),
            *lamports,
            Vec::new(),
            false,
        )),
        AccountState::SystemEmpty => Ok(acct_state(
            address,
            system_program::id().to_bytes(),
            0,
            Vec::new(),
            false,
        )),
        AccountState::SystemAllocated { data } => Ok(acct_state(
            address,
            system_program::id().to_bytes(),
            0,
            data.clone(),
            false,
        )),
        AccountState::NonceInitialized {
            authority,
            extra_lamports,
        } => {
            let lamports = nonce_rent_exempt_lamports().saturating_add(*extra_lamports);
            Ok(nonce_account(
                address,
                lamports,
                nonce_initialized_data(authority)?,
            ))
        }
        AccountState::NonceInitializedLowRent { authority } => Ok(nonce_account(
            address,
            nonce_rent_exempt_lamports().saturating_sub(1),
            nonce_initialized_data(authority)?,
        )),
        AccountState::NonceUninitialized => Ok(nonce_account(
            address,
            nonce_rent_exempt_lamports(),
            nonce_uninitialized_data()?,
        )),
        AccountState::SysvarRent => rent_sysvar_account(address),
        AccountState::SysvarRecentBlockhashes => recent_blockhashes_sysvar_account(address),
        AccountState::SysvarRecentBlockhashesEmpty => {
            recent_blockhashes_empty_sysvar_account(address)
        }
        AccountState::ForeignEmpty { owner } => Ok(acct_state(
            address,
            address_expr_bytes(owner),
            0,
            Vec::new(),
            false,
        )),
        AccountState::ForeignData {
            lamports,
            data,
            owner,
        } => Ok(acct_state(
            address,
            address_expr_bytes(owner),
            *lamports,
            data.clone(),
            false,
        )),
    }
}

fn system_program_account(address: [u8; 32]) -> il::Result<AcctState> {
    let data = b"system_program".to_vec();
    let lamports = solana_rent::Rent::default()
        .minimum_balance(data.len())
        .max(1);
    Ok(acct_state(
        address,
        native_loader::id().to_bytes(),
        lamports,
        data,
        true,
    ))
}

fn clock_sysvar_account(address: [u8; 32]) -> il::Result<AcctState> {
    let data = bincode::serialize(&solana_clock::Clock::default())
        .map_err(|error| IlError::new(format!("serializing clock sysvar: {error}")))?;
    Ok(sysvar_account(address, data))
}

fn rent_sysvar_account(address: [u8; 32]) -> il::Result<AcctState> {
    let data = bincode::serialize(&solana_rent::Rent::default())
        .map_err(|error| IlError::new(format!("serializing rent sysvar: {error}")))?;
    Ok(sysvar_account(address, data))
}

#[allow(deprecated)]
fn recent_blockhashes_sysvar_account(address: [u8; 32]) -> il::Result<AcctState> {
    use {
        solana_hash::Hash,
        solana_sysvar::recent_blockhashes::{IterItem, MAX_ENTRIES, RecentBlockhashes},
    };

    let blockhash = Hash::default();
    let recent_blockhashes: RecentBlockhashes = (0..MAX_ENTRIES)
        .map(|_| IterItem(0u64, &blockhash, 0))
        .collect();
    let data = bincode::serialize(&recent_blockhashes)
        .map_err(|error| IlError::new(format!("serializing recent blockhashes sysvar: {error}")))?;
    Ok(sysvar_account(address, data))
}

#[allow(deprecated)]
fn recent_blockhashes_empty_sysvar_account(address: [u8; 32]) -> il::Result<AcctState> {
    let data = bincode::serialize(&solana_sysvar::recent_blockhashes::RecentBlockhashes::default())
        .map_err(|error| {
            IlError::new(format!(
                "serializing empty recent blockhashes sysvar: {error}"
            ))
        })?;
    Ok(sysvar_account(address, data))
}

fn nonce_rent_exempt_lamports() -> u64 {
    solana_rent::Rent::default().minimum_balance(NonceState::size())
}

fn nonce_account(address: [u8; 32], lamports: u64, data: Vec<u8>) -> AcctState {
    acct_state(
        address,
        system_program::id().to_bytes(),
        lamports,
        data,
        false,
    )
}

fn nonce_initialized_data(authority: &AddressExpr) -> il::Result<Vec<u8>> {
    let authority = Pubkey::from(address_expr_bytes(authority));
    let state = NonceVersions::new(NonceState::Initialized(NonceData::new(
        authority,
        DurableNonce::default(),
        0,
    )));
    nonce_data_bytes(&state)
}

fn nonce_uninitialized_data() -> il::Result<Vec<u8>> {
    nonce_data_bytes(&NonceVersions::new(NonceState::Uninitialized))
}

fn nonce_data_bytes(state: &NonceVersions) -> il::Result<Vec<u8>> {
    let serialized = bincode::serialize(state)
        .map_err(|error| IlError::new(format!("serializing nonce state: {error}")))?;
    let mut data = vec![0; NonceState::size()];
    if serialized.len() > data.len() {
        return Err(IlError::new(format!(
            "serialized nonce state is {} bytes, expected at most {}",
            serialized.len(),
            data.len()
        )));
    }
    data[..serialized.len()].copy_from_slice(&serialized);
    Ok(data)
}

fn sysvar_account(address: [u8; 32], data: Vec<u8>) -> AcctState {
    acct_state(address, sysvar::id().to_bytes(), 1, data, false)
}

fn acct_state(
    address: [u8; 32],
    owner: [u8; 32],
    lamports: u64,
    data: Vec<u8>,
    executable: bool,
) -> AcctState {
    AcctState {
        address: address.to_vec(),
        lamports,
        data,
        executable,
        owner: owner.to_vec(),
    }
}

fn print_instr_context(index: usize, context: &InstrContext) {
    eprintln!("InstrContext[{index}] {{");
    eprintln!("  program_id: {}", hex(&context.program_id));
    eprintln!("  cu_avail: {}", context.cu_avail);
    eprintln!("  data: {}", hex(&context.data));
    eprintln!("  accounts:");
    for (account_index, account) in context.accounts.iter().enumerate() {
        eprintln!("    [{account_index}] {{");
        eprintln!("      address: {}", hex(&account.address));
        eprintln!("      owner: {}", hex(&account.owner));
        eprintln!("      lamports: {}", account.lamports);
        eprintln!("      executable: {}", account.executable);
        eprintln!("      data_len: {}", account.data.len());
        eprintln!("      data_prefix: {}", hex_prefix(&account.data, 32));
        eprintln!("    }}");
    }
    eprintln!("  instr_accounts:");
    for account in &context.instr_accounts {
        eprintln!(
            "    index={} writable={} signer={}",
            account.index, account.is_writable, account.is_signer
        );
    }
    eprintln!("}}");
}

fn write_instr_context(index: usize, context: &InstrContext) -> il::Result<PathBuf> {
    let stem = format!(
        "fuzz-{}-{}-ix{index}",
        std::process::id(),
        compiler::now_nanos()
    );
    let path = compiler::temp_artifact_dir().join(format!("{stem}.instr.pb"));
    std::fs::write(&path, context.encode_to_vec())?;
    Ok(path)
}

fn display_path(path: &std::path::Path) -> String {
    path.to_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_prefix(bytes: &[u8], limit: usize) -> String {
    let prefix_len = bytes.len().min(limit);
    let mut prefix = hex(&bytes[..prefix_len]);
    if bytes.len() > limit {
        prefix.push_str("...");
    }
    prefix
}

#[cfg(test)]
mod tests {
    use {super::*, crate::lower};

    fn instr_account(context: &InstrContext, instr_index: usize) -> &AcctState {
        let account_index = context.instr_accounts[instr_index].index as usize;
        &context.accounts[account_index]
    }

    #[test]
    fn builds_context_with_requested_cu_budget() {
        let lowered = lower::lower_il(
            "LoadU64 1\nTransfer | ;\n  (account:0, true, true),\n  (account:1, true, false)\n",
        )
        .unwrap();
        let contexts = lowered_to_instr_contexts(&lowered).unwrap();

        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].cu_avail, 1_400_000);
        assert_eq!(contexts[0].instr_accounts.len(), 2);
        assert_eq!(contexts[0].program_id, system_program::id().to_bytes());
    }

    #[test]
    fn writes_context_as_protobuf() {
        let lowered = lower::lower_il(
            "LoadU64 1\nTransfer | ;\n  (account:0, true, true),\n  (account:1, true, false)\n",
        )
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let path = write_instr_context(0, context).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let decoded = InstrContext::decode(bytes.as_slice()).unwrap();
        let _ = std::fs::remove_file(path);

        assert!(decoded == context.clone());
    }

    #[test]
    fn applies_system_empty_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState account:0 SystemEmpty\nLoadU64 1\nTransfer | ;\n  (account:0, true, \
             true),\n  (account:1, true, false)\n",
        )
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let source = instr_account(context, 0);

        assert_eq!(source.owner, system_program::id().to_bytes());
        assert_eq!(source.lamports, 0);
        assert!(source.data.is_empty());
    }

    #[test]
    fn applies_foreign_data_account_state() {
        let owner = "0101010101010101010101010101010101010101010101010101010101010101";
        let lowered = lower::lower_il(&format!(
            "LoadAccountState account:0 ForeignData 7 hex:deadbeef {owner}\nLoadU64 1\nTransfer | \
             ;\n  (account:0, true, true),\n  (account:1, true, false)\n"
        ))
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let source = instr_account(context, 0);

        assert_eq!(source.owner, vec![1; 32]);
        assert_eq!(source.lamports, 7);
        assert_eq!(source.data, [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn applies_initialized_nonce_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState account:0 NonceInitialized account:1 123\nLoadU64 \
             1\nWithdrawNonceAccount | ;\n  (account:0, true, false),\n  (account:2, true, \
             false),\n  (sysvar:recent_blockhashes, false, false),\n  (sysvar:rent, false, \
             false),\n  (account:1, false, true)\n",
        )
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let nonce = instr_account(context, 0);

        assert_eq!(nonce.owner, system_program::id().to_bytes());
        assert_eq!(nonce.lamports, nonce_rent_exempt_lamports() + 123);
        assert_eq!(nonce.data.len(), NonceState::size());
        assert_ne!(nonce.data, vec![0; NonceState::size()]);
    }

    #[test]
    fn applies_empty_recent_blockhashes_sysvar_state() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:recent_blockhashes \
             SysvarRecentBlockhashesEmpty\nAdvanceNonceAccount | ;\n  (account:0, true, false),\n  \
             (sysvar:recent_blockhashes, false, false),\n  (account:1, false, true)\n",
        )
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let recent_blockhashes = instr_account(context, 1);
        let populated =
            recent_blockhashes_sysvar_account(sysvar::recent_blockhashes::id().to_bytes()).unwrap();

        assert_eq!(recent_blockhashes.owner, sysvar::id().to_bytes());
        assert!(recent_blockhashes.data.len() < populated.data.len());
    }
}
