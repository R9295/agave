use {
    crate::{
        il::{
            AccountMetaArg, AccountState, AccountStateTarget, AddressExpr, IlError, Program,
            PubkeyBytes, Result, Statement, Value, parse_account_index_token,
            parse_address_literal, parse_program, parse_string, parse_u64,
        },
        template::TEMPLATE,
    },
    solana_system_interface::instruction::SystemInstruction,
    std::{
        collections::{HashMap, VecDeque},
        fmt::Write as _,
    },
};

#[derive(Default)]
struct Env {
    values: HashMap<String, Value>,
    u8s: VecDeque<u8>,
    u64s: VecDeque<u64>,
    strings: VecDeque<String>,
    addresses: VecDeque<AddressExpr>,
}

impl Env {
    fn insert(&mut self, name: Option<&str>, value: Value) {
        match &value {
            Value::U8(value) => self.u8s.push_back(*value),
            Value::U64(value) => self.u64s.push_back(*value),
            Value::String(value) => self.strings.push_back(value.clone()),
            Value::Address(value) => self.addresses.push_back(value.clone()),
            Value::Account(_) => {}
        }
        if let Some(name) = name {
            self.values.insert(name.to_owned(), value);
        }
    }

    fn resolve(&self, token: &str) -> Option<&Value> {
        self.values.get(token)
    }

    fn take_u64(&mut self, line: usize, field: &str) -> Result<u64> {
        self.u64s
            .pop_front()
            .ok_or_else(|| IlError::line(line, format!("missing u64 operand for {field}")))
    }

    fn take_u8(&mut self, line: usize, field: &str) -> Result<u8> {
        self.u8s
            .pop_front()
            .ok_or_else(|| IlError::line(line, format!("missing u8 operand for {field}")))
    }

    fn take_string(&mut self, line: usize, field: &str) -> Result<String> {
        self.strings
            .pop_front()
            .ok_or_else(|| IlError::line(line, format!("missing string operand for {field}")))
    }

    fn take_address(&mut self, line: usize, field: &str) -> Result<AddressExpr> {
        self.addresses
            .pop_front()
            .ok_or_else(|| IlError::line(line, format!("missing address operand for {field}")))
    }
}

#[derive(Debug)]
pub(crate) struct LoweredProgram {
    pub(crate) invocations: Vec<Invocation>,
    pub(crate) account_states: Vec<LoweredAccountState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredAccountState {
    pub(crate) target: AccountStateTarget,
    pub(crate) state: AccountState,
}

#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) data: Vec<u8>,
    pub(crate) patches: Vec<AddressPatch>,
    pub(crate) metas: Vec<AccountMeta>,
    pub(crate) kind: InvocationKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InvocationKind {
    System,
    AccountResize {
        account_index: usize,
        new_len: u64,
    },
    WriteAccountData {
        account_index: usize,
        offset: u64,
        len: u64,
        value: u8,
    },
}

#[derive(Debug)]
pub(crate) struct AddressPatch {
    pub(crate) offset: usize,
    pub(crate) source: AddressExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AccountMeta {
    pub(crate) pubkey: MetaPubkey,
    pub(crate) is_writable: bool,
    pub(crate) is_signer: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetaPubkey {
    Account(usize),
    ProgramId,
    Literal(PubkeyBytes),
}

pub(crate) fn lower_il(source: &str) -> Result<LoweredProgram> {
    let program = parse_program(source)?;
    lower_program(&program)
}

#[cfg(test)]
pub(crate) fn lower_il_to_c(source: &str) -> Result<String> {
    let lowered = lower_il(source)?;
    lowered_to_c(&lowered)
}

pub(crate) fn lowered_to_c(program: &LoweredProgram) -> Result<String> {
    assemble_c(&render_user_body(program)?)
}

fn lower_program(program: &Program) -> Result<LoweredProgram> {
    let mut env = Env::default();
    let mut invocations = Vec::new();
    let mut account_states = Vec::new();
    for statement in &program.statements {
        match statement {
            Statement::Load { line, name, value } => {
                let _ = line;
                env.insert(name.as_deref(), value.clone());
            }
            Statement::AccountState { target, state } => {
                account_states.push(LoweredAccountState {
                    target: target.clone(),
                    state: state.clone(),
                });
            }
            Statement::Invoke {
                line,
                op,
                args,
                accounts,
            } => {
                invocations.push(lower_invocation(
                    *line,
                    op,
                    args,
                    accounts.as_deref(),
                    &mut env,
                )?);
            }
        }
    }
    Ok(LoweredProgram {
        invocations,
        account_states,
    })
}

fn lower_invocation(
    line: usize,
    op: &str,
    args: &[String],
    account_args: Option<&[AccountMetaArg]>,
    env: &mut Env,
) -> Result<Invocation> {
    let account_args = account_args.ok_or_else(|| {
        IlError::line(
            line,
            format!("{op} is missing explicit account list; add `; ...`"),
        )
    })?;
    match op {
        "CreateAccount" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            let (owner, owner_patch) = lower_address_field(20, owner);
            system_invocation(
                SystemInstruction::CreateAccount {
                    lamports,
                    space,
                    owner,
                },
                vec![owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "Assign" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let owner = resolver.address("owner")?;
            let (owner, owner_patch) = lower_address_field(4, owner);
            system_invocation(
                SystemInstruction::Assign { owner },
                vec![owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "Transfer" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            system_invocation(
                SystemInstruction::Transfer { lamports },
                Vec::new(),
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "CreateAccountWithSeed" => {
            ensure_arg_count(line, op, args, &[0, 5])?;
            let mut resolver = Resolver::new(line, args, env);
            let base = resolver.address("base")?;
            let seed = resolver.string("seed")?;
            let lamports = resolver.u64("lamports")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            let owner_offset = checked_add(line, 60, seed.len())?;
            let (base, base_patch) = lower_address_field(4, base);
            let (owner, owner_patch) = lower_address_field(owner_offset, owner);
            system_invocation(
                SystemInstruction::CreateAccountWithSeed {
                    base,
                    seed,
                    lamports,
                    space,
                    owner,
                },
                vec![base_patch, owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "AdvanceNonceAccount" => {
            ensure_arg_count(line, op, args, &[0])?;
            system_invocation(
                SystemInstruction::AdvanceNonceAccount,
                Vec::new(),
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "WithdrawNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            system_invocation(
                SystemInstruction::WithdrawNonceAccount(lamports),
                Vec::new(),
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "InitializeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let authority = resolver.address("authority")?;
            let (authority, authority_patch) = lower_address_field(4, authority);
            system_invocation(
                SystemInstruction::InitializeNonceAccount(authority),
                vec![authority_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "AuthorizeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let authority = resolver.address("authority")?;
            let (authority, authority_patch) = lower_address_field(4, authority);
            system_invocation(
                SystemInstruction::AuthorizeNonceAccount(authority),
                vec![authority_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "Allocate" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let space = resolver.u64("space")?;
            system_invocation(
                SystemInstruction::Allocate { space },
                Vec::new(),
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "AllocateWithSeed" => {
            ensure_arg_count(line, op, args, &[0, 4])?;
            let mut resolver = Resolver::new(line, args, env);
            let base = resolver.address("base")?;
            let seed = resolver.string("seed")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            let owner_offset = checked_add(line, 52, seed.len())?;
            let (base, base_patch) = lower_address_field(4, base);
            let (owner, owner_patch) = lower_address_field(owner_offset, owner);
            system_invocation(
                SystemInstruction::AllocateWithSeed {
                    base,
                    seed,
                    space,
                    owner,
                },
                vec![base_patch, owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "AssignWithSeed" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let base = resolver.address("base")?;
            let seed = resolver.string("seed")?;
            let owner = resolver.address("owner")?;
            let owner_offset = checked_add(line, 44, seed.len())?;
            let (base, base_patch) = lower_address_field(4, base);
            let (owner, owner_patch) = lower_address_field(owner_offset, owner);
            system_invocation(
                SystemInstruction::AssignWithSeed { base, seed, owner },
                vec![base_patch, owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "TransferWithSeed" => lower_transfer_with_seed(line, op, args, account_args, env),
        "AccountResize" => lower_account_resize(line, op, args, account_args, env),
        "WriteAccountData" => lower_write_account_data(line, op, args, account_args, env),
        "UpgradeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0])?;
            system_invocation(
                SystemInstruction::UpgradeNonceAccount,
                Vec::new(),
                resolve_account_metas(line, env, account_args)?,
            )
        }
        "CreateAccountAllowPrefund" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            let (owner, owner_patch) = lower_address_field(20, owner);
            system_invocation(
                SystemInstruction::CreateAccountAllowPrefund {
                    lamports,
                    space,
                    owner,
                },
                vec![owner_patch],
                resolve_account_metas(line, env, account_args)?,
            )
        }
        _ => Err(IlError::line(
            line,
            format!("unknown IL instruction `{op}`"),
        )),
    }
}

fn lower_transfer_with_seed(
    line: usize,
    op: &str,
    args: &[String],
    account_args: &[AccountMetaArg],
    env: &mut Env,
) -> Result<Invocation> {
    ensure_arg_count(line, op, args, &[0, 3])?;
    let mut resolver = Resolver::new(line, args, env);
    let lamports = resolver.u64("lamports")?;
    let seed = resolver.string("from_seed")?;
    let owner = resolver.address("from_owner")?;
    let owner_offset = checked_add(line, 20, seed.len())?;
    let (owner, owner_patch) = lower_address_field(owner_offset, owner);
    system_invocation(
        SystemInstruction::TransferWithSeed {
            lamports,
            from_seed: seed,
            from_owner: owner,
        },
        vec![owner_patch],
        resolve_account_metas(line, env, account_args)?,
    )
}

fn lower_account_resize(
    line: usize,
    op: &str,
    args: &[String],
    account_args: &[AccountMetaArg],
    env: &mut Env,
) -> Result<Invocation> {
    ensure_arg_count(line, op, args, &[0, 1])?;
    let mut resolver = Resolver::new(line, args, env);
    let new_len = resolver.u64("new_len")?;
    let metas = resolve_account_metas(line, env, account_args)?;
    let account_index = single_account_meta_index(line, op, &metas)?;
    Ok(Invocation {
        data: Vec::new(),
        patches: Vec::new(),
        metas,
        kind: InvocationKind::AccountResize {
            account_index,
            new_len,
        },
    })
}

fn lower_write_account_data(
    line: usize,
    op: &str,
    args: &[String],
    account_args: &[AccountMetaArg],
    env: &mut Env,
) -> Result<Invocation> {
    ensure_arg_count(line, op, args, &[0, 3])?;
    let mut resolver = Resolver::new(line, args, env);
    let offset = resolver.u64("offset")?;
    let len = resolver.u64("len")?;
    let value = resolver.u8("value")?;
    let metas = resolve_account_metas(line, env, account_args)?;
    let account_index = single_account_meta_index(line, op, &metas)?;
    Ok(Invocation {
        data: Vec::new(),
        patches: Vec::new(),
        metas,
        kind: InvocationKind::WriteAccountData {
            account_index,
            offset,
            len,
            value,
        },
    })
}

fn single_account_meta_index(line: usize, op: &str, metas: &[AccountMeta]) -> Result<usize> {
    let [meta] = metas else {
        return Err(IlError::line(
            line,
            format!("{op} expects exactly one account meta, got {}", metas.len()),
        ));
    };
    let MetaPubkey::Account(account_index) = meta.pubkey else {
        return Err(IlError::line(
            line,
            format!("{op} target must be account:N"),
        ));
    };
    Ok(account_index)
}

struct Resolver<'a, 'b> {
    line: usize,
    args: &'a [String],
    position: usize,
    env: &'b mut Env,
}

impl<'a, 'b> Resolver<'a, 'b> {
    fn new(line: usize, args: &'a [String], env: &'b mut Env) -> Self {
        Self {
            line,
            args,
            position: 0,
            env,
        }
    }

    fn u64(&mut self, field: &str) -> Result<u64> {
        if self.args.is_empty() {
            return self.env.take_u64(self.line, field);
        }
        let token = self.next_token(field)?;
        resolve_u64_token(self.line, self.env, token, field)
    }

    fn u8(&mut self, field: &str) -> Result<u8> {
        if self.args.is_empty() {
            return self.env.take_u8(self.line, field);
        }
        let token = self.next_token(field)?;
        resolve_u8_token(self.line, self.env, token, field)
    }

    fn string(&mut self, field: &str) -> Result<String> {
        if self.args.is_empty() {
            return self.env.take_string(self.line, field);
        }
        let token = self.next_token(field)?;
        resolve_string_token(self.line, self.env, token, field)
    }

    fn address(&mut self, field: &str) -> Result<AddressExpr> {
        if self.args.is_empty() {
            return self.env.take_address(self.line, field);
        }
        let token = self.next_token(field)?;
        resolve_address_token(self.line, self.env, token, field)
    }

    fn next_token(&mut self, field: &str) -> Result<&'a str> {
        let Some(token) = self.args.get(self.position) else {
            return Err(IlError::line(
                self.line,
                format!("missing operand for {field}"),
            ));
        };
        self.position += 1;
        Ok(token)
    }
}

fn ensure_arg_count(line: usize, op: &str, args: &[String], allowed: &[usize]) -> Result<()> {
    if allowed.contains(&args.len()) {
        return Ok(());
    }
    let allowed = allowed
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Err(IlError::line(
        line,
        format!(
            "{op} expects one of [{allowed}] operands, got {}",
            args.len()
        ),
    ))
}

fn system_invocation(
    instruction: SystemInstruction,
    patches: Vec<Option<AddressPatch>>,
    metas: Vec<AccountMeta>,
) -> Result<Invocation> {
    let data = bincode::serialize(&instruction)
        .map_err(|error| IlError::new(format!("serializing system instruction: {error}")))?;
    let patches = patches.into_iter().flatten().collect();
    Ok(Invocation {
        data,
        patches,
        metas,
        kind: InvocationKind::System,
    })
}

fn lower_address_field(
    offset: usize,
    source: AddressExpr,
) -> (solana_address::Address, Option<AddressPatch>) {
    let address = match &source {
        AddressExpr::Static(pubkey) => pubkey.to_address(),
        AddressExpr::AccountKey(_) | AddressExpr::ProgramId => PubkeyBytes::SYSTEM.to_address(),
    };
    let patch = match source {
        AddressExpr::Static(_) => None,
        AddressExpr::AccountKey(_) | AddressExpr::ProgramId => {
            Some(AddressPatch { offset, source })
        }
    };
    (address, patch)
}

fn checked_add(line: usize, left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or_else(|| IlError::line(line, "instruction offset overflow"))
}

fn resolve_account_metas(
    line: usize,
    env: &Env,
    account_args: &[AccountMetaArg],
) -> Result<Vec<AccountMeta>> {
    account_args
        .iter()
        .map(|account| {
            Ok(AccountMeta {
                pubkey: resolve_meta_token(line, env, &account.pubkey, "account meta")?,
                is_writable: account.is_writable,
                is_signer: account.is_signer,
            })
        })
        .collect()
}

fn resolve_u64_token(line: usize, env: &Env, token: &str, field: &str) -> Result<u64> {
    if let Some(value) = env.resolve(token) {
        return match value {
            Value::U8(value) => Ok(u64::from(*value)),
            Value::U64(value) => Ok(*value),
            _ => Err(IlError::line(
                line,
                format!("{field} expects u64, `{token}` is {:?}", value.kind()),
            )),
        };
    }
    parse_u64(line, token)
}

fn resolve_u8_token(line: usize, env: &Env, token: &str, field: &str) -> Result<u8> {
    if let Some(value) = env.resolve(token) {
        return match value {
            Value::U8(value) => Ok(*value),
            _ => Err(IlError::line(
                line,
                format!("{field} expects u8, `{token}` is {:?}", value.kind()),
            )),
        };
    }
    let value = parse_u64(line, token)?;
    u8::try_from(value)
        .map_err(|_| IlError::line(line, format!("{field} literal `{token}` is out of range")))
}

fn resolve_string_token(line: usize, env: &Env, token: &str, field: &str) -> Result<String> {
    if let Some(value) = env.resolve(token) {
        return match value {
            Value::String(value) => Ok(value.clone()),
            _ => Err(IlError::line(
                line,
                format!("{field} expects string, `{token}` is {:?}", value.kind()),
            )),
        };
    }
    parse_string(line, token)
}

fn resolve_address_token(line: usize, env: &Env, token: &str, field: &str) -> Result<AddressExpr> {
    if let Some(value) = env.resolve(token) {
        return match value {
            Value::Address(value) => Ok(value.clone()),
            Value::Account(value) => Ok(AddressExpr::AccountKey(*value)),
            _ => Err(IlError::line(
                line,
                format!("{field} expects address, `{token}` is {:?}", value.kind()),
            )),
        };
    }
    parse_address_literal(line, token)
}

fn resolve_meta_token(line: usize, env: &Env, token: &str, field: &str) -> Result<MetaPubkey> {
    if let Some(value) = env.resolve(token) {
        return match value {
            Value::Address(value) => Ok(meta_from_address_expr(value)),
            Value::Account(value) => Ok(MetaPubkey::Account(*value)),
            _ => Err(IlError::line(
                line,
                format!(
                    "{field} expects account/address, `{token}` is {:?}",
                    value.kind()
                ),
            )),
        };
    }
    if let Some(index) = parse_account_index_token(token) {
        if index == 0 {
            return Err(IlError::line(
                line,
                "account:0 is reserved for the implicit harness account",
            ));
        }
        return Ok(MetaPubkey::Account(index));
    }
    parse_address_literal(line, token).map(|address| meta_from_address_expr(&address))
}

fn meta_from_address_expr(address: &AddressExpr) -> MetaPubkey {
    match address {
        AddressExpr::Static(pubkey) => MetaPubkey::Literal(*pubkey),
        AddressExpr::AccountKey(index) => MetaPubkey::Account(*index),
        AddressExpr::ProgramId => MetaPubkey::ProgramId,
    }
}

fn render_user_body(program: &LoweredProgram) -> Result<String> {
    let mut output = String::new();
    output.push_str("static void fuzz_il_main(SolParameters *params) {\n");
    output.push_str("    if (params == 0) {\n        return;\n    }\n");
    for (index, invocation) in program.invocations.iter().enumerate() {
        render_invocation(&mut output, index, invocation)?;
    }
    output.push_str("}\n");
    Ok(output)
}

fn render_invocation(output: &mut String, index: usize, invocation: &Invocation) -> Result<()> {
    match invocation.kind {
        InvocationKind::System => render_system_invocation(output, index, invocation),
        InvocationKind::AccountResize {
            account_index,
            new_len,
        } => render_account_resize(output, account_index, new_len),
        InvocationKind::WriteAccountData {
            account_index,
            offset,
            len,
            value,
        } => render_write_account_data(output, account_index, offset, len, value),
    }
}

fn render_system_invocation(
    output: &mut String,
    index: usize,
    invocation: &Invocation,
) -> Result<()> {
    let min_accounts = invocation
        .metas
        .iter()
        .filter_map(|meta| match meta.pubkey {
            MetaPubkey::Account(account_index) => account_index.checked_add(1),
            MetaPubkey::ProgramId | MetaPubkey::Literal(_) => None,
        })
        .chain(
            invocation
                .patches
                .iter()
                .filter_map(|patch| match patch.source {
                    AddressExpr::AccountKey(account_index) => account_index.checked_add(1),
                    AddressExpr::Static(_) | AddressExpr::ProgramId => None,
                }),
        )
        .max()
        .unwrap_or(0)
        .max(1);

    writeln!(output, "    if (params->ka_num >= {min_accounts}) {{")
        .map_err(|error| IlError::new(error.to_string()))?;
    let data = c_byte_array(&invocation.data);
    writeln!(
        output,
        "        uint8_t ix{index}_data[{}] = {{{data}}};",
        invocation.data.len()
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    for patch in &invocation.patches {
        match patch.source {
            AddressExpr::AccountKey(account_index) => writeln!(
                output,
                "        sol_memcpy_(ix{index}_data + {}, params->ka[{account_index}].key->x, 32);",
                patch.offset
            ),
            AddressExpr::ProgramId => writeln!(
                output,
                "        sol_memcpy_(ix{index}_data + {}, params->program_id->x, 32);",
                patch.offset
            ),
            AddressExpr::Static(_) => Ok(()),
        }
        .map_err(|error| IlError::new(error.to_string()))?;
    }

    for (meta_index, meta) in invocation.metas.iter().enumerate() {
        if let MetaPubkey::Literal(pubkey) = meta.pubkey {
            writeln!(
                output,
                "        SolPubkey ix{index}_meta{meta_index}_pubkey = {{ .x = {{{}}} }};",
                pubkey.c_initializer()
            )
            .map_err(|error| IlError::new(error.to_string()))?;
        }
    }

    writeln!(
        output,
        "        SolAccountMeta ix{index}_metas[{}];",
        invocation.metas.len()
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    for (meta_index, meta) in invocation.metas.iter().enumerate() {
        let pubkey = render_meta_pubkey(index, meta_index, &meta.pubkey);
        let writable = u8::from(meta.is_writable);
        let signer = u8::from(meta.is_signer);
        writeln!(
            output,
            "        ix{index}_metas[{meta_index}] = (SolAccountMeta){{ .pubkey = {pubkey}, \
             .is_writable = {writable}, .is_signer = {signer} }};"
        )
        .map_err(|error| IlError::new(error.to_string()))?;
    }
    writeln!(
        output,
        "        SolInstruction ix{index} = (SolInstruction){{ .program_id = (SolPubkey \
         *)&SYSTEM_PROGRAM_ID, .accounts = ix{index}_metas, .account_len = {}, .data = \
         ix{index}_data, .data_len = {} }};",
        invocation.metas.len(),
        invocation.data.len()
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    writeln!(
        output,
        "        (void)sol_invoke_signed_c(&ix{index}, params->ka, (int)params->ka_num, 0, 0);"
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    output.push_str("    }\n");
    Ok(())
}

fn render_account_resize(output: &mut String, account_index: usize, new_len: u64) -> Result<()> {
    let min_accounts = account_index
        .checked_add(1)
        .ok_or_else(|| IlError::new("account index overflow"))?;
    writeln!(output, "    if (params->ka_num >= {min_accounts}) {{")
        .map_err(|error| IlError::new(error.to_string()))?;
    writeln!(
        output,
        "        account_resize(&params->ka[{account_index}], {new_len});"
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    output.push_str("    }\n");
    Ok(())
}

fn render_write_account_data(
    output: &mut String,
    account_index: usize,
    offset: u64,
    len: u64,
    value: u8,
) -> Result<()> {
    let min_accounts = account_index
        .checked_add(1)
        .ok_or_else(|| IlError::new("account index overflow"))?;
    writeln!(output, "    if (params->ka_num >= {min_accounts}) {{")
        .map_err(|error| IlError::new(error.to_string()))?;
    writeln!(
        output,
        "        write_account_data(&params->ka[{account_index}], {offset}, {len}, {value});"
    )
    .map_err(|error| IlError::new(error.to_string()))?;
    output.push_str("    }\n");
    Ok(())
}

fn render_meta_pubkey(invocation_index: usize, meta_index: usize, pubkey: &MetaPubkey) -> String {
    match pubkey {
        MetaPubkey::Account(account_index) => format!("params->ka[{account_index}].key"),
        MetaPubkey::ProgramId => "(SolPubkey *)params->program_id".to_owned(),
        MetaPubkey::Literal(_) => format!("&ix{invocation_index}_meta{meta_index}_pubkey"),
    }
}

fn c_byte_array(data: &[u8]) -> String {
    data.iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn assemble_c(user_body: &str) -> Result<String> {
    let marker = "/* Entrypoint */";
    let Some(marker_index) = TEMPLATE.find(marker) else {
        return Err(IlError::new("template missing entrypoint marker"));
    };
    let mut source = String::new();
    source.push_str(&TEMPLATE[..marker_index]);
    source.push_str(user_body);
    source.push('\n');
    source.push_str(&TEMPLATE[marker_index..]);

    let return_marker = "    return 0;\n}";
    let Some(return_index) = source.rfind(return_marker) else {
        return Err(IlError::new("template missing entrypoint return marker"));
    };
    source.replace_range(
        return_index..return_index + return_marker.len(),
        "    fuzz_il_main(&params);\n    return 0;\n}",
    );
    Ok(source)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        solana_address::Address,
        std::{
            collections::BTreeSet,
            path::{Path, PathBuf},
        },
    };

    const KNOWN_SYSVARS: [&str; 3] = ["sysvar:clock", "sysvar:recent_blockhashes", "sysvar:rent"];
    const REQUIRED_HARNESS_SYSVARS: [&str; 2] = ["sysvar:clock", "sysvar:rent"];

    fn instruction_data(source: &str) -> Vec<u8> {
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        lowered.invocations[0].data.clone()
    }

    #[test]
    fn parses_loads_and_pipe_operands() {
        let source = r#"
            LoadU64 lamports = 7
            LoadU64 space = 9
            LoadAddress owner = system
            CreateAccount | lamports, space, owner ;
              (account:1, true, true),
              (account:2, true, true)
        "#;
        let data = instruction_data(source);
        assert_eq!(
            data,
            bincode::serialize(&SystemInstruction::CreateAccount {
                lamports: 7,
                space: 9,
                owner: Address::from([0; 32]),
            })
            .unwrap()
        );
    }

    #[test]
    fn consumes_implicit_typed_operands() {
        let source = r#"
            LoadU64 3
            Transfer | ;
              (account:1, true, true),
              (account:2, true, false)
        "#;
        assert_eq!(
            instruction_data(source),
            bincode::serialize(&SystemInstruction::Transfer { lamports: 3 }).unwrap()
        );
    }

    #[test]
    fn patches_dynamic_owner_address() {
        let source = r#"
            LoadU64 lamports = 1
            LoadU64 space = 2
            LoadAddress owner = account:4
            CreateAccount | lamports, space, owner ;
              (account:1, true, true),
              (account:2, true, true)
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(lowered.invocations[0].patches[0].offset, 20);
        assert_eq!(
            lowered.invocations[0].patches[0].source,
            AddressExpr::AccountKey(4)
        );
    }

    #[test]
    fn transfer_with_seed_uses_explicit_account_section() {
        let source = r#"
            LoadU64 lamports = 5
            LoadString seed = "abc"
            LoadAddress owner = system
            TransferWithSeed | lamports, seed, owner ;
              (account:1, true, false),
              (account:1, false, true),
              (account:3, true, false)
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(
            lowered.invocations[0].data,
            bincode::serialize(&SystemInstruction::TransferWithSeed {
                lamports: 5,
                from_seed: "abc".to_owned(),
                from_owner: Address::from([0; 32]),
            })
            .unwrap()
        );
        assert_eq!(
            lowered.invocations[0].metas[0].pubkey,
            MetaPubkey::Account(1)
        );
        assert_eq!(
            lowered.invocations[0].metas[2].pubkey,
            MetaPubkey::Account(3)
        );
    }

    #[test]
    fn load_account_names_meta_operands() {
        let source = r#"
            LoadAccount from = account:1
            LoadAccount to = account:3
            LoadString seed = "abc"
            TransferWithSeed | 5, seed, system ;
              (from, true, false),
              (from, false, true),
              (to, true, false)
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(
            lowered.invocations[0].metas[0].pubkey,
            MetaPubkey::Account(1)
        );
        assert_eq!(
            lowered.invocations[0].metas[2].pubkey,
            MetaPubkey::Account(3)
        );
    }

    #[test]
    fn account_resize_renders_direct_account_mutation() {
        let source = r#"
            LoadU64 new_len = 7
            AccountResize | new_len ;
              (account:1, true, false)
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(
            lowered.invocations[0].kind,
            InvocationKind::AccountResize {
                account_index: 1,
                new_len: 7,
            }
        );
        assert_eq!(lowered.invocations[0].metas.len(), 1);
        let c_source = lowered_to_c(&lowered).unwrap();
        assert!(c_source.contains("account_resize(&params->ka[1], 7);"));
        assert!(!c_source.contains("(void)sol_invoke_signed_c"));
    }

    #[test]
    fn account_resize_requires_one_account_target() {
        assert!(lower_il_to_c("LoadU64 7\nAccountResize | ;\n  (system, true, false)\n").is_err());
        assert!(
            lower_il_to_c(
                "LoadU64 7\nAccountResize | ;\n  (account:1, true, false),\n  (account:2, true, \
                 false)\n"
            )
            .is_err()
        );
    }

    #[test]
    fn write_account_data_renders_direct_memset() {
        let source = r#"
            LoadU64 offset = 2
            LoadU64 len = 3
            LoadU8 value = 255
            WriteAccountData | offset, len, value ;
              (account:1, true, false)
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(
            lowered.invocations[0].kind,
            InvocationKind::WriteAccountData {
                account_index: 1,
                offset: 2,
                len: 3,
                value: 255,
            }
        );
        let c_source = lowered_to_c(&lowered).unwrap();
        assert!(c_source.contains("write_account_data(&params->ka[1], 2, 3, 255);"));
        assert!(!c_source.contains("(void)sol_invoke_signed_c"));
    }

    #[test]
    fn write_account_data_consumes_implicit_typed_operands() {
        let source = r#"
            LoadU64 4
            LoadU64 5
            LoadU8 6
            WriteAccountData | ;
              (account:1, true, false)
        "#;
        let c_source = lower_il_to_c(source).unwrap();
        assert!(c_source.contains("write_account_data(&params->ka[1], 4, 5, 6);"));
    }

    #[test]
    fn rejects_noncanonical_tokens() {
        assert!(lower_il_to_c("loadu64 1\n").is_err());
        assert!(parse_address_literal(1, "system_program").is_err());
        assert!(parse_address_literal(1, "ka:0").is_err());
        assert!(
            lower_il_to_c(
                "LoadAccountState account:1 SystemEmpty\nTransfer | ;\n  (account:1, true, true)\n"
            )
            .is_err()
        );
        assert!(
            lower_il_to_c(
                "LoadU8 from = 0\nTransferWithSeed | 5, \"abc\", system ;\n  (from, true, \
                 false),\n  (account:2, true, false)\n"
            )
            .is_err()
        );
    }

    #[test]
    fn emitted_c_is_spliced_into_entrypoint() {
        let c_source = lower_il_to_c(
            "LoadU64 1\nTransfer | ;\n  (account:1, true, true),\n  (account:2, true, false)\n",
        )
        .unwrap();
        assert!(c_source.contains("static void fuzz_il_main"));
        assert!(c_source.contains("fuzz_il_main(&params);"));
        assert!(c_source.contains("sol_invoke_signed_c"));
    }

    #[test]
    fn lowers_all_testcases() {
        let paths = testcase_paths();
        assert!(!paths.is_empty());
        for path in paths {
            let source = std::fs::read_to_string(&path).unwrap();
            lower_il_to_c(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }

    #[test]
    fn testcases_declare_referenced_accounts() {
        for path in testcase_paths() {
            let source = std::fs::read_to_string(&path).unwrap();
            let referenced_accounts = account_indices_in_source(&source);
            let declared_accounts = declared_account_indices(&source);
            let missing_accounts = referenced_accounts
                .difference(&declared_accounts)
                .copied()
                .collect::<Vec<_>>();
            assert!(
                missing_accounts.is_empty(),
                "{} missing LoadAccountState declarations for accounts: {:?}",
                path.display(),
                missing_accounts
            );

            let mut referenced_sysvars = sysvars_in_source(&source);
            referenced_sysvars.extend(REQUIRED_HARNESS_SYSVARS);
            let declared_sysvars = declared_sysvars(&source);
            let missing_sysvars = referenced_sysvars
                .difference(&declared_sysvars)
                .copied()
                .collect::<Vec<_>>();
            assert!(
                missing_sysvars.is_empty(),
                "{} missing LoadAccountState declarations for sysvars: {:?}",
                path.display(),
                missing_sysvars
            );
        }
    }

    fn testcase_paths() -> Vec<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testcases");
        let mut paths = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("il"))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    fn account_indices_in_source(source: &str) -> BTreeSet<usize> {
        let mut indices = BTreeSet::new();
        let mut rest = source;
        while let Some(offset) = rest.find("account:") {
            let after_prefix = &rest[offset + "account:".len()..];
            let digit_len = after_prefix.bytes().take_while(u8::is_ascii_digit).count();
            if digit_len > 0 {
                if let Ok(index) = after_prefix[..digit_len].parse::<usize>() {
                    indices.insert(index);
                }
            }
            rest = &after_prefix[digit_len..];
        }
        indices
    }

    fn declared_account_indices(source: &str) -> BTreeSet<usize> {
        source
            .lines()
            .filter_map(|line| {
                let rest = line
                    .trim_start()
                    .strip_prefix("LoadAccountState account:")?;
                let digit_len = rest.bytes().take_while(u8::is_ascii_digit).count();
                rest[..digit_len].parse::<usize>().ok()
            })
            .collect()
    }

    fn sysvars_in_source(source: &str) -> BTreeSet<&'static str> {
        KNOWN_SYSVARS
            .into_iter()
            .filter(|sysvar| source.contains(sysvar))
            .collect()
    }

    fn declared_sysvars(source: &str) -> BTreeSet<&'static str> {
        KNOWN_SYSVARS
            .into_iter()
            .filter(|sysvar| {
                source.lines().any(|line| {
                    line.trim_start()
                        .starts_with(&format!("LoadAccountState {sysvar} "))
                })
            })
            .collect()
    }
}
