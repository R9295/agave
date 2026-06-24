use {
    crate::{
        compiler,
        il::{
            self, AccountState, AccountStateTarget, AddressExpr, IlError, harness_program_id_bytes,
            harness2_account_id_bytes,
        },
        lower::{Invocation, InvocationKind, LoweredAccountState, LoweredProgram, MetaPubkey},
    },
    prost::Message,
    protosol::protos::{AcctState, InstrAcct, InstrContext},
    solana_loader_v3_interface::{get_program_data_address, state::UpgradeableLoaderState},
    solana_pubkey::Pubkey,
    solana_sdk_ids::{bpf_loader_upgradeable, native_loader, system_program, sysvar},
    std::{collections::HashMap, path::PathBuf},
};

pub(crate) fn print_lowered(program: &LoweredProgram, elf_bytes: &[u8]) -> il::Result<()> {
    let context = lowered_to_instr_context(program, elf_bytes)?;
    print_instr_context(0, &context);
    let path = write_instr_context(0, &context)?;
    eprintln!("InstrContext[0] protobuf: {}", display_path(&path));
    Ok(())
}

pub(crate) fn lowered_to_instr_context(
    program: &LoweredProgram,
    elf_bytes: &[u8],
) -> il::Result<InstrContext> {
    let account_states = AccountStateOverrides::from_lowered(&program.account_states)?;
    program_to_instr_context(program, &account_states, elf_bytes)
}

fn program_to_instr_context(
    program: &LoweredProgram,
    account_states: &AccountStateOverrides,
    elf_bytes: &[u8],
) -> il::Result<InstrContext> {
    let requirements = AccountRequirements::from_program(program)?;
    let mut accounts = Vec::new();
    let mut instr_accounts = Vec::new();
    let mut by_address = HashMap::<[u8; 32], u32>::new();

    let (harness_account, programdata_account) = harness_program_accounts(elf_bytes)?;
    let harness_index = push_account(&mut accounts, &mut by_address, harness_account)?;
    let _programdata_index = push_account(&mut accounts, &mut by_address, programdata_account)?;
    instr_accounts.push(InstrAcct {
        index: harness_index,
        is_writable: requirements.harness_flags.is_writable,
        is_signer: requirements.harness_flags.is_signer,
    });

    for account_index in 1..=requirements.max_account_index {
        let state = account_states.for_account(account_index).ok_or_else(|| {
            IlError::new(format!(
                "missing LoadAccountState for account:{account_index}"
            ))
        })?;
        let address = synthetic_account_key(account_index);
        let context_account_index = push_account(
            &mut accounts,
            &mut by_address,
            account_state(address, state)?,
        )?;
        let flags = requirements
            .account_flags
            .get(&account_index)
            .copied()
            .unwrap_or_default();
        instr_accounts.push(InstrAcct {
            index: context_account_index,
            is_writable: flags.is_writable,
            is_signer: flags.is_signer,
        });
    }

    for address_meta in requirements.address_metas {
        if by_address.contains_key(&address_meta.address) {
            continue;
        }
        let context_account_index = if address_meta.address == system_program::id().to_bytes() {
            push_account(&mut accounts, &mut by_address, system_program_account())
        } else {
            let state = account_states
                .for_address(&address_meta.address)
                .ok_or_else(|| {
                    IlError::new(format!(
                        "missing LoadAccountState for account meta {} ({})",
                        address_meta.label,
                        hex(&address_meta.address)
                    ))
                })?;
            push_account(
                &mut accounts,
                &mut by_address,
                account_state(address_meta.address, state)?,
            )
        }?;
        instr_accounts.push(InstrAcct {
            index: context_account_index,
            is_writable: address_meta.flags.is_writable,
            is_signer: address_meta.flags.is_signer,
        });
    }
    push_system_program_caller_account(&mut accounts, &mut instr_accounts, &mut by_address)?;
    push_required_harness_sysvars(&mut accounts, &mut by_address, account_states)?;

    Ok(InstrContext {
        program_id: harness_program_id_bytes().to_vec(),
        accounts,
        instr_accounts,
        data: Vec::new(),
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
    fn from_lowered(states: &[LoweredAccountState]) -> il::Result<Self> {
        let mut overrides = Self::default();
        let mut by_resolved_address = HashMap::<[u8; 32], String>::new();
        for state in states {
            let resolved_address = account_state_target_bytes(&state.target);
            if resolved_address == harness_program_id_bytes() {
                return Err(IlError::new(
                    "the harness account is implicit; do not declare LoadAccountState for it",
                ));
            }
            let target_label = account_state_target_label(&state.target);
            match &state.target {
                AccountStateTarget::Account(index) => {
                    if *index == 0 {
                        return Err(IlError::new(
                            "account:0 is reserved for the implicit harness account",
                        ));
                    }
                    if overrides
                        .by_account
                        .insert(*index, state.state.clone())
                        .is_some()
                    {
                        return Err(IlError::new(format!(
                            "duplicate LoadAccountState for account:{index}"
                        )));
                    }
                }
                AccountStateTarget::Address(address) => {
                    let address_bytes = address_expr_bytes(address);
                    if overrides
                        .by_address
                        .insert(address_bytes, state.state.clone())
                        .is_some()
                    {
                        return Err(IlError::new(format!(
                            "duplicate LoadAccountState for address {}",
                            hex(&address_bytes)
                        )));
                    }
                }
            }
            if let Some(previous_target) =
                by_resolved_address.insert(resolved_address, target_label.clone())
            {
                return Err(IlError::new(format!(
                    "duplicate LoadAccountState for {target_label}; {previous_target} resolves to \
                     the same address {}",
                    hex(&resolved_address)
                )));
            }
        }
        Ok(overrides)
    }

    fn for_account(&self, index: usize) -> Option<&AccountState> {
        self.by_account.get(&index)
    }

    fn for_address(&self, address: &[u8; 32]) -> Option<&AccountState> {
        self.by_address.get(address)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AccountFlags {
    is_writable: bool,
    is_signer: bool,
}

impl AccountFlags {
    fn include(&mut self, is_writable: bool, is_signer: bool) {
        self.is_writable |= is_writable;
        self.is_signer |= is_signer;
    }
}

#[derive(Debug)]
struct AddressMeta {
    address: [u8; 32],
    label: String,
    flags: AccountFlags,
}

#[derive(Debug, Default)]
struct AccountRequirements {
    max_account_index: usize,
    account_flags: HashMap<usize, AccountFlags>,
    address_metas: Vec<AddressMeta>,
    address_meta_indexes: HashMap<[u8; 32], usize>,
    harness_flags: AccountFlags,
}

impl AccountRequirements {
    fn from_program(program: &LoweredProgram) -> il::Result<Self> {
        let mut requirements = Self::default();
        for invocation in &program.invocations {
            requirements.include_invocation(invocation)?;
        }
        Ok(requirements)
    }

    fn include_invocation(&mut self, invocation: &Invocation) -> il::Result<()> {
        for meta in &invocation.metas {
            match meta.pubkey {
                MetaPubkey::Account(index) => {
                    self.include_account(index, meta.is_writable, meta.is_signer)?;
                }
                MetaPubkey::ProgramId | MetaPubkey::Literal(_) => {
                    let address = meta_pubkey_bytes(&meta.pubkey)?;
                    self.include_address(
                        address,
                        meta_pubkey_label(&meta.pubkey),
                        meta.is_writable,
                        meta.is_signer,
                    );
                }
            }
        }
        for patch in &invocation.patches {
            if let AddressExpr::AccountKey(index) = patch.source {
                self.include_account(index, false, false)?;
            }
        }
        if let InvocationKind::SetAccountOwner {
            owner: MetaPubkey::Account(index),
            ..
        } = invocation.kind
        {
            self.include_account(index, false, false)?;
        }
        Ok(())
    }

    fn include_account(
        &mut self,
        index: usize,
        is_writable: bool,
        is_signer: bool,
    ) -> il::Result<()> {
        if index == 0 {
            return Err(IlError::new(
                "account:0 is reserved for the implicit harness account",
            ));
        }
        self.max_account_index = self.max_account_index.max(index);
        self.account_flags
            .entry(index)
            .or_default()
            .include(is_writable, is_signer);
        Ok(())
    }

    fn include_address(
        &mut self,
        address: [u8; 32],
        label: String,
        is_writable: bool,
        is_signer: bool,
    ) {
        if address == harness_program_id_bytes() {
            self.harness_flags.include(is_writable, is_signer);
            return;
        }
        if let Some(index) = self.address_meta_indexes.get(&address).copied() {
            self.address_metas[index]
                .flags
                .include(is_writable, is_signer);
            return;
        }
        let index = self.address_metas.len();
        self.address_metas.push(AddressMeta {
            address,
            label,
            flags: AccountFlags {
                is_writable,
                is_signer,
            },
        });
        self.address_meta_indexes.insert(address, index);
    }
}

fn meta_pubkey_bytes(pubkey: &MetaPubkey) -> il::Result<[u8; 32]> {
    match pubkey {
        MetaPubkey::Account(index) => Ok(synthetic_account_key(*index)),
        MetaPubkey::ProgramId => Ok(harness_program_id_bytes()),
        MetaPubkey::Literal(pubkey) => Ok(pubkey.0),
    }
}

fn meta_pubkey_label(pubkey: &MetaPubkey) -> String {
    match pubkey {
        MetaPubkey::Account(index) => format!("account:{index}"),
        MetaPubkey::ProgramId => "program".to_owned(),
        MetaPubkey::Literal(pubkey) => address_label(pubkey.0),
    }
}

fn account_state_target_bytes(target: &AccountStateTarget) -> [u8; 32] {
    match target {
        AccountStateTarget::Account(index) => synthetic_account_key(*index),
        AccountStateTarget::Address(address) => address_expr_bytes(address),
    }
}

fn account_state_target_label(target: &AccountStateTarget) -> String {
    match target {
        AccountStateTarget::Account(index) => format!("account:{index}"),
        AccountStateTarget::Address(address) => address_label(address_expr_bytes(address)),
    }
}

fn address_expr_bytes(address: &AddressExpr) -> [u8; 32] {
    match address {
        AddressExpr::Static(pubkey) => pubkey.0,
        AddressExpr::AccountKey(index) => synthetic_account_key(*index),
        AddressExpr::ProgramId => harness_program_id_bytes(),
    }
}

fn address_label(address: [u8; 32]) -> String {
    if address == harness2_account_id_bytes() {
        "harness2".to_owned()
    } else {
        format!("address {}", hex(&address))
    }
}

fn synthetic_account_key(index: usize) -> [u8; 32] {
    let mut key = [0; 32];
    key[..15].copy_from_slice(b"fuzz-il-account");
    key[24..].copy_from_slice(&(index as u64).to_le_bytes());
    key
}

fn push_account(
    accounts: &mut Vec<AcctState>,
    by_address: &mut HashMap<[u8; 32], u32>,
    account: AcctState,
) -> il::Result<u32> {
    let address = account_address(&account)?;
    if let Some(index) = by_address.get(&address) {
        return Ok(*index);
    }
    let index = u32::try_from(accounts.len())
        .map_err(|_| IlError::new("too many InstrContext accounts"))?;
    accounts.push(account);
    by_address.insert(address, index);
    Ok(index)
}

fn push_required_harness_sysvars(
    accounts: &mut Vec<AcctState>,
    by_address: &mut HashMap<[u8; 32], u32>,
    account_states: &AccountStateOverrides,
) -> il::Result<()> {
    push_required_harness_sysvar(
        accounts,
        by_address,
        account_states,
        sysvar::clock::id().to_bytes(),
        "sysvar:clock",
    )?;
    push_required_harness_sysvar(
        accounts,
        by_address,
        account_states,
        sysvar::rent::id().to_bytes(),
        "sysvar:rent",
    )
}

fn push_required_harness_sysvar(
    accounts: &mut Vec<AcctState>,
    by_address: &mut HashMap<[u8; 32], u32>,
    account_states: &AccountStateOverrides,
    address: [u8; 32],
    label: &str,
) -> il::Result<()> {
    if by_address.contains_key(&address) {
        return Ok(());
    }
    let state = account_states.for_address(&address).ok_or_else(|| {
        IlError::new(format!(
            "missing LoadAccountState for required harness sysvar {label} ({})",
            hex(&address)
        ))
    })?;
    push_account(accounts, by_address, account_state(address, state)?)?;
    Ok(())
}

fn push_system_program_caller_account(
    accounts: &mut Vec<AcctState>,
    instr_accounts: &mut Vec<InstrAcct>,
    by_address: &mut HashMap<[u8; 32], u32>,
) -> il::Result<()> {
    let index = push_account(accounts, by_address, system_program_account())?;
    if instr_accounts.iter().any(|account| account.index == index) {
        return Ok(());
    }
    instr_accounts.push(InstrAcct {
        index,
        is_writable: false,
        is_signer: false,
    });
    Ok(())
}

fn account_address(account: &AcctState) -> il::Result<[u8; 32]> {
    account
        .address
        .as_slice()
        .try_into()
        .map_err(|_| IlError::new("account address must be 32 bytes"))
}

fn harness_program_accounts(elf_bytes: &[u8]) -> il::Result<(AcctState, AcctState)> {
    let loader_id = bpf_loader_upgradeable::id();
    let program_id = Pubkey::from(harness_program_id_bytes());
    let programdata_address = get_program_data_address(&program_id);
    let rent = solana_rent::Rent::default();

    let program_data = bincode::serialize(&UpgradeableLoaderState::Program {
        programdata_address,
    })
    .map_err(|error| IlError::new(format!("serializing harness program account: {error}")))?;
    let program_lamports = rent
        .minimum_balance(UpgradeableLoaderState::size_of_program())
        .max(1);

    let mut programdata_data = bincode::serialize(&UpgradeableLoaderState::ProgramData {
        slot: 0,
        upgrade_authority_address: Some(Pubkey::default()),
    })
    .map_err(|error| IlError::new(format!("serializing harness programdata account: {error}")))?;
    let programdata_metadata_len = UpgradeableLoaderState::size_of_programdata_metadata();
    if programdata_data.len() > programdata_metadata_len {
        return Err(IlError::new(format!(
            "serialized harness programdata metadata is {} bytes, expected at most {}",
            programdata_data.len(),
            programdata_metadata_len
        )));
    }
    programdata_data.resize(programdata_metadata_len, 0);
    programdata_data.extend_from_slice(elf_bytes);
    let programdata_lamports = rent
        .minimum_balance(programdata_metadata_len + elf_bytes.len())
        .max(1);

    Ok((
        acct_state(
            harness_program_id_bytes(),
            loader_id.to_bytes(),
            program_lamports,
            program_data,
            true,
        ),
        acct_state(
            programdata_address.to_bytes(),
            loader_id.to_bytes(),
            programdata_lamports,
            programdata_data,
            false,
        ),
    ))
}

fn system_program_account() -> AcctState {
    let data = b"system_program".to_vec();
    let lamports = solana_rent::Rent::default()
        .minimum_balance(data.len())
        .max(1);
    acct_state(
        system_program::id().to_bytes(),
        native_loader::id().to_bytes(),
        lamports,
        data,
        true,
    )
}

fn account_state(address: [u8; 32], state: &AccountState) -> il::Result<AcctState> {
    match state {
        AccountState::Explicit {
            data,
            owner,
            lamports,
        } => Ok(acct_state(
            address,
            address_expr_bytes(owner),
            *lamports,
            data.clone(),
            false,
        )),
        AccountState::SysvarClock => {
            require_state_address(address, sysvar::clock::id().to_bytes(), "SysvarClock")?;
            clock_sysvar_account(address)
        }
        AccountState::SysvarRent => {
            require_state_address(address, sysvar::rent::id().to_bytes(), "SysvarRent")?;
            rent_sysvar_account(address)
        }
        AccountState::SysvarRecentBlockhashes => {
            require_state_address(
                address,
                sysvar::recent_blockhashes::id().to_bytes(),
                "SysvarRecentBlockhashes",
            )?;
            recent_blockhashes_sysvar_account(address)
        }
        AccountState::SysvarRecentBlockhashesEmpty => {
            require_state_address(
                address,
                sysvar::recent_blockhashes::id().to_bytes(),
                "SysvarRecentBlockhashesEmpty",
            )?;
            recent_blockhashes_empty_sysvar_account(address)
        }
    }
}

fn require_state_address(address: [u8; 32], expected: [u8; 32], kind: &str) -> il::Result<()> {
    if address == expected {
        return Ok(());
    }
    Err(IlError::new(format!(
        "{kind} must be declared for address {}, got {}",
        hex(&expected),
        hex(&address)
    )))
}

fn clock_sysvar_account(address: [u8; 32]) -> il::Result<AcctState> {
    let clock = solana_clock::Clock {
        slot: 1,
        ..solana_clock::Clock::default()
    };
    let data = bincode::serialize(&clock)
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

    const ELF_BYTES: &[u8] = b"test-elf";

    fn instr_account(context: &InstrContext, instr_index: usize) -> &AcctState {
        let account_index = context.instr_accounts[instr_index].index as usize;
        &context.accounts[account_index]
    }

    #[test]
    fn builds_context_with_requested_cu_budget() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | zeros:0 | system | 2\nLoadAccountState \
             account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  (account:1, true, \
             true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();

        assert_eq!(context.cu_avail, 1_400_000);
        assert_eq!(context.instr_accounts.len(), 4);
        assert_eq!(context.program_id, harness_program_id_bytes());
        assert_eq!(context.accounts[0].address, harness_program_id_bytes());
        assert_eq!(
            context.accounts[0].owner,
            bpf_loader_upgradeable::id().to_bytes()
        );
        assert!(context.accounts[0].executable);
        assert!(context.accounts[1].data.ends_with(ELF_BYTES));
        let system_program = instr_account(&context, 3);
        assert_eq!(system_program.address, system_program::id().to_bytes());
        assert_eq!(system_program.owner, native_loader::id().to_bytes());
        assert!(system_program.executable);
    }

    #[test]
    fn writes_context_as_protobuf() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | zeros:0 | system | 2\nLoadAccountState \
             account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  (account:1, true, \
             true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let path = write_instr_context(0, &context).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let decoded = InstrContext::decode(bytes.as_slice()).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(decoded, context);
    }

    #[test]
    fn applies_system_empty_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | zeros:0 | system | 0\nLoadAccountState \
             account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  (account:1, true, \
             true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let source = instr_account(&context, 1);

        assert_eq!(source.owner, system_program::id().to_bytes());
        assert_eq!(source.lamports, 0);
        assert!(source.data.is_empty());
    }

    #[test]
    fn applies_foreign_data_account_state() {
        let owner = "0101010101010101010101010101010101010101010101010101010101010101";
        let lowered = lower::lower_il(&format!(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | hex:deadbeef | {owner} | \
             7\nLoadAccountState account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  \
             (account:1, true, true),\n  (account:2, true, false)\n"
        ))
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let source = instr_account(&context, 1);

        assert_eq!(source.owner, vec![1; 32]);
        assert_eq!(source.lamports, 7);
        assert_eq!(source.data, [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn applies_explicit_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | hex:deadbeef | system | \
             123\nLoadAccountState account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  \
             (account:1, true, true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let account = instr_account(&context, 1);

        assert_eq!(account.owner, system_program::id().to_bytes());
        assert_eq!(account.lamports, 123);
        assert_eq!(account.data, [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn set_account_owner_source_account_is_required() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState account:1 | zeros:0 | harness | 1\nLoadAccountState \
             account:2 | zeros:0 | system | 2\nSetAccountOwner | account:2 ;\n  (account:1, true, \
             false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let owner_source = instr_account(&context, 2);

        assert_eq!(owner_source.address, synthetic_account_key(2));
        assert_eq!(owner_source.lamports, 2);
    }

    #[test]
    fn harness2_alias_is_a_declared_normal_account() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState harness2 | zeros:0 | system | 9\nLoadAccountState \
             account:1 | zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  (harness2, true, \
             true),\n  (account:1, true, false)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let harness2 = instr_account(&context, 2);

        assert_eq!(harness2.address, harness2_account_id_bytes());
        assert_eq!(harness2.owner, system_program::id().to_bytes());
        assert_eq!(harness2.lamports, 9);
    }

    #[test]
    fn applies_empty_recent_blockhashes_sysvar_state() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:clock SysvarClock\nLoadAccountState sysvar:rent \
             SysvarRent\nLoadAccountState sysvar:recent_blockhashes \
             SysvarRecentBlockhashesEmpty\nLoadAccountState account:1 | zeros:0 | system | \
             0\nLoadAccountState account:2 | zeros:0 | system | 1\nAdvanceNonceAccount | ;\n  \
             (account:1, true, false),\n  (sysvar:recent_blockhashes, false, false),\n  \
             (account:2, false, true)\n",
        )
        .unwrap();
        let context = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap();
        let recent_blockhashes = instr_account(&context, 3);
        let populated =
            recent_blockhashes_sysvar_account(sysvar::recent_blockhashes::id().to_bytes()).unwrap();

        assert_eq!(recent_blockhashes.owner, sysvar::id().to_bytes());
        assert!(recent_blockhashes.data.len() < populated.data.len());
    }

    #[test]
    fn rejects_missing_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState account:1 | zeros:0 | system | 2\nLoadU64 1\nTransfer | ;\n  \
             (account:1, true, true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let error = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap_err();

        assert!(error.to_string().contains("missing LoadAccountState"));
        assert!(error.to_string().contains("account:2"));
    }

    #[test]
    fn rejects_missing_required_harness_sysvar() {
        let lowered = lower::lower_il(
            "LoadAccountState sysvar:rent SysvarRent\nLoadAccountState account:1 | zeros:0 | \
             system | 2\nLoadAccountState account:2 | zeros:0 | system | 0\nLoadU64 1\nTransfer | \
             ;\n  (account:1, true, true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        let error = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap_err();

        assert!(error.to_string().contains("required harness sysvar"));
        assert!(error.to_string().contains("sysvar:clock"));
    }

    #[test]
    fn rejects_duplicate_account_state() {
        let lowered = lower::lower_il(
            "LoadAccountState account:1 | zeros:0 | system | 2\nLoadAccountState account:1 | \
             zeros:0 | system | 0\nLoadU64 1\nTransfer | ;\n  (account:1, true, true)\n",
        )
        .unwrap();
        let error = lowered_to_instr_context(&lowered, ELF_BYTES).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("duplicate LoadAccountState for account:1")
        );
    }

    #[test]
    fn rejects_explicit_harness_account_state() {
        let error = lower::lower_il(
            "LoadAccountState account:0 | zeros:0 | system | 0\nTransfer | ;\n  (account:1, true, \
             true)\n",
        )
        .unwrap_err();

        assert!(error.to_string().contains("account:0 is reserved"));
    }
}
