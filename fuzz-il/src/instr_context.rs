use {
    crate::{
        compiler,
        il::{self, AddressExpr, IlError},
        lower::{Invocation, LoweredProgram, MetaPubkey},
    },
    prost::Message,
    protosol::protos::{AcctState, InstrAcct, InstrContext},
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
    program
        .invocations
        .iter()
        .map(invocation_to_instr_context)
        .collect()
}

fn invocation_to_instr_context(invocation: &Invocation) -> il::Result<InstrContext> {
    let mut accounts = ContextAccounts::default();
    let mut instr_accounts = Vec::with_capacity(invocation.metas.len());

    for meta in &invocation.metas {
        let address = meta_pubkey_bytes(&meta.pubkey)?;
        let account_index = accounts.index_for(address)?;
        instr_accounts.push(InstrAcct {
            index: account_index,
            is_writable: meta.is_writable,
            is_signer: meta.is_signer,
        });
    }

    accounts.index_for(system_program::id().to_bytes())?;
    accounts.index_for(sysvar::clock::id().to_bytes())?;
    accounts.index_for(sysvar::rent::id().to_bytes())?;

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
struct ContextAccounts {
    accounts: Vec<AcctState>,
    by_address: HashMap<[u8; 32], u32>,
}

impl ContextAccounts {
    fn index_for(&mut self, address: [u8; 32]) -> il::Result<u32> {
        if let Some(index) = self.by_address.get(&address) {
            return Ok(*index);
        }
        let index = u32::try_from(self.accounts.len())
            .map_err(|_| IlError::new("too many InstrContext accounts"))?;
        self.accounts.push(account_state(address)?);
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

fn account_state(address: [u8; 32]) -> il::Result<AcctState> {
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
        eprintln!(
            "    [{account_index}] address={} owner={} lamports={} executable={} data_len={} \
             data_prefix={}",
            hex(&account.address),
            hex(&account.owner),
            account.lamports,
            account.executable,
            account.data.len(),
            hex_prefix(&account.data, 32)
        );
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

    #[test]
    fn builds_context_with_requested_cu_budget() {
        let lowered = lower::lower_il(
            "LoadU64 1\nTransfer | ; (account:0, true, true), (account:1, true, false)\n",
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
            "LoadU64 1\nTransfer | ; (account:0, true, true), (account:1, true, false)\n",
        )
        .unwrap();
        let context = &lowered_to_instr_contexts(&lowered).unwrap()[0];
        let path = write_instr_context(0, context).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let decoded = InstrContext::decode(bytes.as_slice()).unwrap();
        let _ = std::fs::remove_file(path);

        assert!(decoded == context.clone());
    }
}
