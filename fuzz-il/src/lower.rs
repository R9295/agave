use {
    crate::{
        il::{
            AddressExpr, IlError, Program, PubkeyBytes, Result, Statement, Value,
            parse_account_index_token, parse_address_literal, parse_program, parse_string,
            parse_u64,
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
struct LoweredProgram {
    invocations: Vec<Invocation>,
}

#[derive(Debug)]
struct Invocation {
    data: Vec<u8>,
    patches: Vec<AddressPatch>,
    metas: Vec<AccountMeta>,
}

#[derive(Debug)]
struct AddressPatch {
    offset: usize,
    source: AddressExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AccountMeta {
    pubkey: MetaPubkey,
    is_writable: bool,
    is_signer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MetaPubkey {
    Account(usize),
    ProgramId,
    Known(&'static str),
    Literal(PubkeyBytes),
}

pub(crate) fn lower_il_to_c(source: &str) -> Result<String> {
    let program = parse_program(source)?;
    let lowered = lower_program(&program)?;
    assemble_c(&render_user_body(&lowered)?)
}

fn lower_program(program: &Program) -> Result<LoweredProgram> {
    let mut env = Env::default();
    let mut invocations = Vec::new();
    for statement in &program.statements {
        match statement {
            Statement::Load { line, name, value } => {
                let _ = line;
                env.insert(name.as_deref(), value.clone());
            }
            Statement::Invoke { line, op, args } => {
                invocations.push(lower_invocation(*line, op, args, &mut env)?);
            }
        }
    }
    Ok(LoweredProgram { invocations })
}

fn lower_invocation(line: usize, op: &str, args: &[String], env: &mut Env) -> Result<Invocation> {
    match op {
        "CreateAccount" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            system_invocation(
                SystemInstruction::CreateAccount {
                    lamports,
                    space,
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(20, owner)],
                vec![account(0, true, true), account(1, true, true)],
            )
        }
        "Assign" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let owner = resolver.address("owner")?;
            system_invocation(
                SystemInstruction::Assign {
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(4, owner)],
                vec![account(0, true, true)],
            )
        }
        "Transfer" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            system_invocation(
                SystemInstruction::Transfer { lamports },
                Vec::new(),
                vec![account(0, true, true), account(1, true, false)],
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
            let mut metas = vec![account(0, true, true), account(1, true, false)];
            append_base_meta(&mut metas, &base, 0);
            system_invocation(
                SystemInstruction::CreateAccountWithSeed {
                    base: base.static_or_default().to_address(),
                    seed,
                    lamports,
                    space,
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(4, base), address_patch(owner_offset, owner)],
                metas,
            )
        }
        "AdvanceNonceAccount" => {
            ensure_arg_count(line, op, args, &[0])?;
            system_invocation(
                SystemInstruction::AdvanceNonceAccount,
                Vec::new(),
                vec![
                    account(0, true, false),
                    known("SYSVAR_RECENT_BLOCKHASHES_ID", false, false),
                    account(1, false, true),
                ],
            )
        }
        "WithdrawNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            system_invocation(
                SystemInstruction::WithdrawNonceAccount(lamports),
                Vec::new(),
                vec![
                    account(0, true, false),
                    account(1, true, false),
                    known("SYSVAR_RECENT_BLOCKHASHES_ID", false, false),
                    known("SYSVAR_RENT_ID", false, false),
                    account(2, false, true),
                ],
            )
        }
        "InitializeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let authority = resolver.address("authority")?;
            system_invocation(
                SystemInstruction::InitializeNonceAccount(
                    authority.static_or_default().to_address(),
                ),
                vec![address_patch(4, authority)],
                vec![
                    account(0, true, false),
                    known("SYSVAR_RECENT_BLOCKHASHES_ID", false, false),
                    known("SYSVAR_RENT_ID", false, false),
                ],
            )
        }
        "AuthorizeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let authority = resolver.address("authority")?;
            system_invocation(
                SystemInstruction::AuthorizeNonceAccount(
                    authority.static_or_default().to_address(),
                ),
                vec![address_patch(4, authority)],
                vec![account(0, true, false), account(1, false, true)],
            )
        }
        "Allocate" => {
            ensure_arg_count(line, op, args, &[0, 1])?;
            let mut resolver = Resolver::new(line, args, env);
            let space = resolver.u64("space")?;
            system_invocation(
                SystemInstruction::Allocate { space },
                Vec::new(),
                vec![account(0, true, true)],
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
            let mut metas = vec![account(0, true, false)];
            append_base_meta(&mut metas, &base, usize::MAX);
            system_invocation(
                SystemInstruction::AllocateWithSeed {
                    base: base.static_or_default().to_address(),
                    seed,
                    space,
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(4, base), address_patch(owner_offset, owner)],
                metas,
            )
        }
        "AssignWithSeed" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let base = resolver.address("base")?;
            let seed = resolver.string("seed")?;
            let owner = resolver.address("owner")?;
            let owner_offset = checked_add(line, 44, seed.len())?;
            let mut metas = vec![account(0, true, false)];
            append_base_meta(&mut metas, &base, usize::MAX);
            system_invocation(
                SystemInstruction::AssignWithSeed {
                    base: base.static_or_default().to_address(),
                    seed,
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(4, base), address_patch(owner_offset, owner)],
                metas,
            )
        }
        "TransferWithSeed" => lower_transfer_with_seed(line, op, args, env),
        "UpgradeNonceAccount" => {
            ensure_arg_count(line, op, args, &[0])?;
            system_invocation(
                SystemInstruction::UpgradeNonceAccount,
                Vec::new(),
                vec![account(0, true, false)],
            )
        }
        "CreateAccountAllowPrefund" => {
            ensure_arg_count(line, op, args, &[0, 3])?;
            let mut resolver = Resolver::new(line, args, env);
            let lamports = resolver.u64("lamports")?;
            let space = resolver.u64("space")?;
            let owner = resolver.address("owner")?;
            let mut metas = vec![account(0, true, true)];
            if lamports > 0 {
                metas.push(account(1, true, true));
            }
            system_invocation(
                SystemInstruction::CreateAccountAllowPrefund {
                    lamports,
                    space,
                    owner: owner.static_or_default().to_address(),
                },
                vec![address_patch(20, owner)],
                metas,
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
    env: &mut Env,
) -> Result<Invocation> {
    ensure_arg_count(line, op, args, &[0, 3, 4, 6])?;
    if args.is_empty() {
        let lamports = env.take_u64(line, "lamports")?;
        let seed = env.take_string(line, "from_seed")?;
        let owner = env.take_address(line, "from_owner")?;
        let owner_offset = checked_add(line, 20, seed.len())?;
        return system_invocation(
            SystemInstruction::TransferWithSeed {
                lamports,
                from_seed: seed,
                from_owner: owner.static_or_default().to_address(),
            },
            vec![address_patch(owner_offset, owner)],
            vec![
                account(0, true, false),
                account(1, false, true),
                account(2, true, false),
            ],
        );
    }

    let lamports = resolve_u64_token(line, env, &args[0], "lamports")?;
    let (from_meta, base_meta, to_meta, seed, owner) = match args.len() {
        3 => {
            let from = resolve_meta_token(line, env, &args[1], "from")?;
            let to = resolve_meta_token(line, env, &args[2], "to")?;
            let seed = env.strings.pop_front().unwrap_or_default();
            let owner = env
                .addresses
                .pop_front()
                .unwrap_or(AddressExpr::Static(PubkeyBytes::SYSTEM));
            (from.clone(), from, to, seed, owner)
        }
        4 => {
            let from = resolve_meta_token(line, env, &args[1], "from")?;
            let base = resolve_meta_token(line, env, &args[2], "base")?;
            let to = resolve_meta_token(line, env, &args[3], "to")?;
            let seed = env.strings.pop_front().unwrap_or_default();
            let owner = env
                .addresses
                .pop_front()
                .unwrap_or(AddressExpr::Static(PubkeyBytes::SYSTEM));
            (from, base, to, seed, owner)
        }
        6 => {
            let from = resolve_meta_token(line, env, &args[1], "from")?;
            let base = resolve_meta_token(line, env, &args[2], "base")?;
            let to = resolve_meta_token(line, env, &args[3], "to")?;
            let seed = resolve_string_token(line, env, &args[4], "from_seed")?;
            let owner = resolve_address_token(line, env, &args[5], "from_owner")?;
            (from, base, to, seed, owner)
        }
        _ => {
            return Err(IlError::line(
                line,
                "TransferWithSeed expects 0, 3, 4, or 6 operands",
            ));
        }
    };
    let owner_offset = checked_add(line, 20, seed.len())?;
    system_invocation(
        SystemInstruction::TransferWithSeed {
            lamports,
            from_seed: seed,
            from_owner: owner.static_or_default().to_address(),
        },
        vec![address_patch(owner_offset, owner)],
        vec![
            AccountMeta {
                pubkey: from_meta,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: base_meta,
                is_writable: false,
                is_signer: true,
            },
            AccountMeta {
                pubkey: to_meta,
                is_writable: true,
                is_signer: false,
            },
        ],
    )
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
    })
}

fn address_patch(offset: usize, source: AddressExpr) -> Option<AddressPatch> {
    match source {
        AddressExpr::Static(_) => None,
        AddressExpr::AccountKey(_) | AddressExpr::ProgramId => {
            Some(AddressPatch { offset, source })
        }
    }
}

fn checked_add(line: usize, left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or_else(|| IlError::line(line, "instruction offset overflow"))
}

fn account(index: usize, is_writable: bool, is_signer: bool) -> AccountMeta {
    AccountMeta {
        pubkey: MetaPubkey::Account(index),
        is_writable,
        is_signer,
    }
}

fn known(name: &'static str, is_writable: bool, is_signer: bool) -> AccountMeta {
    AccountMeta {
        pubkey: MetaPubkey::Known(name),
        is_writable,
        is_signer,
    }
}

fn append_base_meta(metas: &mut Vec<AccountMeta>, base: &AddressExpr, funding_index: usize) {
    match base {
        AddressExpr::AccountKey(index) if *index == funding_index => {}
        AddressExpr::AccountKey(index) => metas.push(account(*index, false, true)),
        AddressExpr::ProgramId => metas.push(AccountMeta {
            pubkey: MetaPubkey::ProgramId,
            is_writable: false,
            is_signer: true,
        }),
        AddressExpr::Static(pubkey) => metas.push(AccountMeta {
            pubkey: MetaPubkey::Literal(*pubkey),
            is_writable: false,
            is_signer: true,
        }),
    }
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
            Value::U8(value) => Ok(AddressExpr::AccountKey(usize::from(*value))),
            Value::U64(value) => usize::try_from(*value)
                .map(AddressExpr::AccountKey)
                .map_err(|_| IlError::line(line, format!("{field} account index overflows usize"))),
            Value::Address(value) => Ok(value.clone()),
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
            Value::U8(value) => Ok(MetaPubkey::Account(usize::from(*value))),
            Value::U64(value) => usize::try_from(*value)
                .map(MetaPubkey::Account)
                .map_err(|_| IlError::line(line, format!("{field} account index overflows usize"))),
            Value::Address(value) => Ok(meta_from_address_expr(value)),
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
    let min_accounts = invocation
        .metas
        .iter()
        .filter_map(|meta| match meta.pubkey {
            MetaPubkey::Account(account_index) => account_index.checked_add(1),
            MetaPubkey::ProgramId | MetaPubkey::Known(_) | MetaPubkey::Literal(_) => None,
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
        .unwrap_or(0);

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
            "        ix{index}_metas[{meta_index}] = (SolAccountMeta){{ .pubkey = {pubkey}, .is_writable = {writable}, .is_signer = {signer} }};"
        )
        .map_err(|error| IlError::new(error.to_string()))?;
    }
    writeln!(
        output,
        "        SolInstruction ix{index} = (SolInstruction){{ .program_id = (SolPubkey *)&SYSTEM_PROGRAM_ID, .accounts = ix{index}_metas, .account_len = {}, .data = ix{index}_data, .data_len = {} }};",
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

fn render_meta_pubkey(invocation_index: usize, meta_index: usize, pubkey: &MetaPubkey) -> String {
    match pubkey {
        MetaPubkey::Account(account_index) => format!("params->ka[{account_index}].key"),
        MetaPubkey::ProgramId => "(SolPubkey *)params->program_id".to_owned(),
        MetaPubkey::Known(name) => format!("(SolPubkey *)&{name}"),
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
    use {super::*, solana_address::Address};

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
            CreateAccount | lamports, space, owner
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
            Transfer
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
            LoadAddress owner = account:3
            CreateAccount | lamports, space, owner
        "#;
        let program = parse_program(source).unwrap();
        let lowered = lower_program(&program).unwrap();
        assert_eq!(lowered.invocations[0].patches[0].offset, 20);
        assert_eq!(
            lowered.invocations[0].patches[0].source,
            AddressExpr::AccountKey(3)
        );
    }

    #[test]
    fn transfer_with_seed_accepts_spec_three_operand_form() {
        let source = r#"
            LoadString seed = "abc"
            TransferWithSeed | 5, account:0, account:2
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
            MetaPubkey::Account(0)
        );
        assert_eq!(
            lowered.invocations[0].metas[2].pubkey,
            MetaPubkey::Account(2)
        );
    }

    #[test]
    fn rejects_noncanonical_tokens() {
        assert!(lower_il_to_c("loadu64 1\n").is_err());
        assert!(parse_address_literal(1, "system_program").is_err());
        assert!(parse_address_literal(1, "ka:0").is_err());
    }

    #[test]
    fn emitted_c_is_spliced_into_entrypoint() {
        let c_source = lower_il_to_c("LoadU64 1\nTransfer\n").unwrap();
        assert!(c_source.contains("static void fuzz_il_main"));
        assert!(c_source.contains("fuzz_il_main(&params);"));
        assert!(c_source.contains("sol_invoke_signed_c"));
    }
}
