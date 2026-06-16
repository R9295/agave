use {
    pest::{Parser, iterators::Pair},
    pest_derive::Parser,
    solana_address::Address,
    solana_sdk_ids::sysvar,
    std::{fmt, str::FromStr},
};

pub(crate) type Result<T> = std::result::Result<T, IlError>;

#[derive(Debug)]
pub struct IlError {
    line: Option<usize>,
    message: String,
}

impl IlError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    pub(crate) fn line(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }
}

impl fmt::Display for IlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for IlError {}

impl From<std::io::Error> for IlError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

pub(crate) fn harness_program_id_bytes() -> [u8; 32] {
    let mut k = [0u8; 32];
    k[0] = 0xa1;
    k[1] = 0xb2;
    k[2] = 0xc3;
    k[3] = 0xd4;
    k[28] = 0xde;
    k[29] = 0xad;
    k[30] = 0xbe;
    k[31] = 0xef;
    k
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PubkeyBytes(pub(crate) [u8; 32]);

impl PubkeyBytes {
    pub(crate) const SYSTEM: Self = Self([0; 32]);

    pub(crate) fn to_address(self) -> Address {
        Address::from(self.0)
    }

    pub(crate) fn c_initializer(self) -> String {
        self.0
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AddressExpr {
    Static(PubkeyBytes),
    AccountKey(usize),
    ProgramId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Value {
    U8(u8),
    U64(u64),
    String(String),
    Address(AddressExpr),
    Account(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueKind {
    U8,
    U64,
    String,
    Address,
    Account,
}

impl Value {
    pub(crate) fn kind(&self) -> ValueKind {
        match self {
            Self::U8(_) => ValueKind::U8,
            Self::U64(_) => ValueKind::U64,
            Self::String(_) => ValueKind::String,
            Self::Address(_) => ValueKind::Address,
            Self::Account(_) => ValueKind::Account,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Program {
    pub(crate) statements: Vec<Statement>,
}

#[derive(Debug)]
pub(crate) enum Statement {
    Load {
        line: usize,
        name: Option<String>,
        value: Value,
    },
    AccountState {
        target: AccountStateTarget,
        state: AccountState,
    },
    Invoke {
        line: usize,
        op: String,
        args: Vec<String>,
        accounts: Option<Vec<AccountMetaArg>>,
    },
}

#[derive(Debug)]
pub(crate) struct AccountMetaArg {
    pub(crate) pubkey: String,
    pub(crate) is_writable: bool,
    pub(crate) is_signer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AccountStateTarget {
    Account(usize),
    Address(AddressExpr),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AccountState {
    SystemFunded {
        lamports: u64,
    },
    SystemEmpty,
    SystemAllocated {
        data: Vec<u8>,
    },
    NonceInitialized {
        authority: AddressExpr,
        extra_lamports: u64,
    },
    NonceInitializedLowRent {
        authority: AddressExpr,
    },
    NonceUninitialized,
    SysvarClock,
    SysvarRent,
    SysvarRecentBlockhashes,
    SysvarRecentBlockhashesEmpty,
    ForeignEmpty {
        owner: AddressExpr,
    },
    ForeignData {
        lamports: u64,
        data: Vec<u8>,
        owner: AddressExpr,
    },
}

#[derive(Parser)]
#[grammar = "il.pest"]
struct IlParser;

pub(crate) fn parse_program(source: &str) -> Result<Program> {
    let pairs = IlParser::parse(Rule::program, source).map_err(|error| IlError {
        line: None,
        message: error.to_string(),
    })?;
    let mut program = Program::default();
    for pair in pairs {
        collect_statements(pair, &mut program)?;
    }
    Ok(program)
}

fn load_kind(op: &str) -> Option<ValueKind> {
    match op {
        "LoadU8" => Some(ValueKind::U8),
        "LoadU64" => Some(ValueKind::U64),
        "LoadString" => Some(ValueKind::String),
        "LoadAddress" => Some(ValueKind::Address),
        "LoadAccount" => Some(ValueKind::Account),
        _ => None,
    }
}

fn collect_statements(pair: Pair<'_, Rule>, program: &mut Program) -> Result<()> {
    match pair.as_rule() {
        Rule::account_state_stmt => program.statements.push(parse_account_state_stmt(pair)?),
        Rule::assigned_load => program.statements.push(parse_assigned_load(pair)?),
        Rule::load_stmt => program.statements.push(parse_load_stmt(pair, None)?),
        Rule::invoke_stmt => program.statements.push(parse_invoke_stmt(pair)?),
        _ => {
            for inner in pair.into_inner() {
                collect_statements(inner, program)?;
            }
        }
    }
    Ok(())
}

fn parse_account_state_stmt(pair: Pair<'_, Rule>) -> Result<Statement> {
    let line = pair.as_span().start_pos().line_col().0;
    let mut inner = pair.into_inner();
    let target_token = inner
        .next()
        .ok_or_else(|| IlError::line(line, "account state missing target"))?
        .as_str()
        .to_owned();
    let target = parse_account_state_target(line, &target_token)?;
    let kind = inner
        .next()
        .ok_or_else(|| IlError::line(line, "account state missing kind"))?
        .as_str()
        .to_owned();
    let args = inner
        .next()
        .map(parse_account_state_args)
        .transpose()?
        .unwrap_or_default();
    let state = parse_account_state(line, &kind, &args)?;
    Ok(Statement::AccountState { target, state })
}

fn parse_account_state_target(line: usize, token: &str) -> Result<AccountStateTarget> {
    if let Some(index) = parse_account_index_token(token) {
        if index == 0 {
            return Err(IlError::line(
                line,
                "account:0 is reserved for the implicit harness account",
            ));
        }
        return Ok(AccountStateTarget::Account(index));
    }
    parse_address_literal(line, token).map(AccountStateTarget::Address)
}

fn parse_account_state_args(pair: Pair<'_, Rule>) -> Result<Vec<String>> {
    let line = pair.as_span().start_pos().line_col().0;
    if pair.as_rule() != Rule::account_state_args {
        return Err(IlError::line(line, "invalid account state arguments"));
    }
    Ok(pair
        .into_inner()
        .filter(|pair| pair.as_rule() == Rule::value)
        .map(|pair| pair.as_str().to_owned())
        .collect())
}

fn parse_account_state(line: usize, kind: &str, args: &[String]) -> Result<AccountState> {
    match kind {
        "SystemFunded" => {
            expect_account_state_args(line, kind, args, &[1])?;
            Ok(AccountState::SystemFunded {
                lamports: parse_u64(line, &args[0])?,
            })
        }
        "SystemEmpty" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::SystemEmpty)
        }
        "SystemAllocated" => {
            expect_account_state_args(line, kind, args, &[1])?;
            Ok(AccountState::SystemAllocated {
                data: parse_data_bytes(line, &args[0])?,
            })
        }
        "NonceInitialized" => {
            expect_account_state_args(line, kind, args, &[2])?;
            let authority = parse_address_literal(line, &args[0])?;
            let extra_lamports = parse_u64(line, &args[1])?;
            Ok(AccountState::NonceInitialized {
                authority,
                extra_lamports,
            })
        }
        "NonceInitializedLowRent" => {
            expect_account_state_args(line, kind, args, &[1])?;
            let authority = parse_address_literal(line, &args[0])?;
            Ok(AccountState::NonceInitializedLowRent { authority })
        }
        "NonceUninitialized" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::NonceUninitialized)
        }
        "SysvarClock" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::SysvarClock)
        }
        "SysvarRent" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::SysvarRent)
        }
        "SysvarRecentBlockhashes" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::SysvarRecentBlockhashes)
        }
        "SysvarRecentBlockhashesEmpty" => {
            expect_account_state_args(line, kind, args, &[0])?;
            Ok(AccountState::SysvarRecentBlockhashesEmpty)
        }
        "ForeignEmpty" => {
            expect_account_state_args(line, kind, args, &[1])?;
            let owner = parse_address_literal(line, &args[0])?;
            Ok(AccountState::ForeignEmpty { owner })
        }
        "ForeignData" => {
            expect_account_state_args(line, kind, args, &[3])?;
            Ok(AccountState::ForeignData {
                lamports: parse_u64(line, &args[0])?,
                data: parse_data_bytes(line, &args[1])?,
                owner: parse_address_literal(line, &args[2])?,
            })
        }
        _ => Err(IlError::line(
            line,
            format!("unknown account state kind `{kind}`"),
        )),
    }
}

fn expect_account_state_args(
    line: usize,
    kind: &str,
    args: &[String],
    allowed: &[usize],
) -> Result<()> {
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
            "{kind} expects one of [{allowed}] arguments, got {}",
            args.len()
        ),
    ))
}

fn parse_assigned_load(pair: Pair<'_, Rule>) -> Result<Statement> {
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| IlError::new("assigned load missing name"))?
        .as_str()
        .to_owned();
    let load = inner
        .next()
        .ok_or_else(|| IlError::new("assigned load missing Load* expression"))?;
    parse_load_stmt(load, Some(name))
}

fn parse_load_stmt(pair: Pair<'_, Rule>, assigned_name: Option<String>) -> Result<Statement> {
    let line = pair.as_span().start_pos().line_col().0;
    let mut inner = pair.into_inner();
    let op = inner
        .next()
        .ok_or_else(|| IlError::line(line, "load statement missing opcode"))?;
    let kind = load_kind(op.as_str())
        .ok_or_else(|| IlError::line(line, format!("unknown load opcode `{}`", op.as_str())))?;
    let (name, token) = match inner.next() {
        None => {
            return Err(IlError::line(
                line,
                format!("expected one value for Load{kind:?}"),
            ));
        }
        Some(arg) => parse_load_arg(line, assigned_name, arg)?,
    };
    let value = parse_load_value(line, kind, token)?;
    Ok(Statement::Load { line, name, value })
}

fn parse_load_arg(
    line: usize,
    assigned_name: Option<String>,
    pair: Pair<'_, Rule>,
) -> Result<(Option<String>, String)> {
    match pair.as_rule() {
        Rule::load_named_assignment | Rule::load_named => {
            if assigned_name.is_some() {
                return Err(IlError::line(line, "load statement has multiple names"));
            }
            let mut inner = pair.into_inner();
            let name = inner
                .next()
                .ok_or_else(|| IlError::line(line, "named load missing name"))?
                .as_str()
                .to_owned();
            let value = inner
                .next()
                .ok_or_else(|| IlError::line(line, "named load missing value"))?
                .as_str()
                .to_owned();
            Ok((Some(name), value))
        }
        Rule::load_value => {
            let value = pair
                .into_inner()
                .next()
                .ok_or_else(|| IlError::line(line, "load missing value"))?
                .as_str()
                .to_owned();
            Ok((assigned_name, value))
        }
        Rule::value => Ok((assigned_name, pair.as_str().to_owned())),
        _ => Err(IlError::line(line, "invalid load operand")),
    }
}

fn parse_load_value(line: usize, kind: ValueKind, token: String) -> Result<Value> {
    match kind {
        ValueKind::U8 => parse_u8(line, &token).map(Value::U8),
        ValueKind::U64 => parse_u64(line, &token).map(Value::U64),
        ValueKind::String => parse_string(line, &token).map(Value::String),
        ValueKind::Address => parse_address_literal(line, &token).map(Value::Address),
        ValueKind::Account => parse_account_literal(line, &token).map(Value::Account),
    }
}

fn parse_invoke_stmt(pair: Pair<'_, Rule>) -> Result<Statement> {
    let line = pair.as_span().start_pos().line_col().0;
    let mut inner = pair.into_inner();
    let op = inner
        .next()
        .ok_or_else(|| IlError::line(line, "invoke statement missing opcode"))?
        .as_str()
        .to_owned();
    let (args, accounts) = inner
        .next()
        .map(parse_invoke_args)
        .transpose()?
        .unwrap_or_default();
    Ok(Statement::Invoke {
        line,
        op,
        args,
        accounts,
    })
}

fn parse_invoke_args(pair: Pair<'_, Rule>) -> Result<(Vec<String>, Option<Vec<AccountMetaArg>>)> {
    let line = pair.as_span().start_pos().line_col().0;
    if pair.as_rule() != Rule::invoke_args {
        return Err(IlError::line(line, "invalid invocation arguments"));
    }
    let mut args = Vec::new();
    let mut accounts = None;
    for pair in pair.into_inner() {
        match pair.as_rule() {
            Rule::data_args => args = parse_wrapped_arg_list(pair)?,
            Rule::account_args => accounts = Some(parse_optional_account_args(pair)?),
            _ => return Err(IlError::line(line, "invalid invocation argument section")),
        }
    }
    Ok((args, accounts))
}

fn parse_wrapped_arg_list(pair: Pair<'_, Rule>) -> Result<Vec<String>> {
    let line = pair.as_span().start_pos().line_col().0;
    let mut inner = pair.into_inner();
    let list = inner
        .next()
        .ok_or_else(|| IlError::line(line, "argument section missing values"))?;
    parse_arg_list(list)
}

fn parse_optional_account_args(pair: Pair<'_, Rule>) -> Result<Vec<AccountMetaArg>> {
    let mut inner = pair.into_inner();
    match inner.next() {
        Some(list) => parse_account_meta_list(list),
        None => Ok(Vec::new()),
    }
}

fn parse_account_meta_list(pair: Pair<'_, Rule>) -> Result<Vec<AccountMetaArg>> {
    let line = pair.as_span().start_pos().line_col().0;
    if pair.as_rule() != Rule::account_meta_list {
        return Err(IlError::line(line, "invalid account meta list"));
    }
    pair.into_inner()
        .filter(|pair| pair.as_rule() == Rule::account_meta)
        .map(parse_account_meta)
        .collect()
}

fn parse_account_meta(pair: Pair<'_, Rule>) -> Result<AccountMetaArg> {
    let line = pair.as_span().start_pos().line_col().0;
    let mut inner = pair.into_inner();
    let pubkey = inner
        .next()
        .ok_or_else(|| IlError::line(line, "account meta missing pubkey"))?
        .as_str()
        .to_owned();
    let is_writable = parse_bool_pair(
        line,
        inner
            .next()
            .ok_or_else(|| IlError::line(line, "account meta missing writable flag"))?,
        "writable",
    )?;
    let is_signer = parse_bool_pair(
        line,
        inner
            .next()
            .ok_or_else(|| IlError::line(line, "account meta missing signer flag"))?,
        "signer",
    )?;
    Ok(AccountMetaArg {
        pubkey,
        is_writable,
        is_signer,
    })
}

fn parse_bool_pair(line: usize, pair: Pair<'_, Rule>, field: &str) -> Result<bool> {
    match pair.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(IlError::line(
            line,
            format!("account meta {field} flag must be true or false, got `{value}`"),
        )),
    }
}

fn parse_arg_list(pair: Pair<'_, Rule>) -> Result<Vec<String>> {
    let line = pair.as_span().start_pos().line_col().0;
    if pair.as_rule() != Rule::arg_list {
        return Err(IlError::line(line, "invalid argument list"));
    }
    Ok(pair
        .into_inner()
        .filter(|pair| pair.as_rule() == Rule::value)
        .map(|pair| pair.as_str().to_owned())
        .collect())
}

fn parse_u8(line: usize, token: &str) -> Result<u8> {
    let value = parse_u64(line, token)?;
    u8::try_from(value)
        .map_err(|_| IlError::line(line, format!("u8 literal `{token}` is out of range")))
}

pub(crate) fn parse_u64(line: usize, token: &str) -> Result<u64> {
    let token = token.replace('_', "");
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16)
            .map_err(|_| IlError::line(line, format!("invalid u64 literal `0x{hex}`")));
    }
    token
        .parse::<u64>()
        .map_err(|_| IlError::line(line, format!("invalid u64 literal `{token}`")))
}

pub(crate) fn parse_string(line: usize, token: &str) -> Result<String> {
    let quoted = (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with('\'') && token.ends_with('\''));
    if !quoted {
        return Ok(token.to_owned());
    }
    let quote = token.as_bytes()[0] as char;
    let inner = &token[1..token.len() - 1];
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(IlError::line(line, "trailing escape in string literal"));
        };
        match escaped {
            '\\' => output.push('\\'),
            '"' if quote == '"' => output.push('"'),
            '\'' if quote == '\'' => output.push('\''),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '0' => output.push('\0'),
            other => {
                output.push('\\');
                output.push(other);
            }
        }
    }
    Ok(output)
}

fn parse_data_bytes(line: usize, token: &str) -> Result<Vec<u8>> {
    if let Some(len) = token
        .strip_prefix("zeros:")
        .or_else(|| token.strip_prefix("zero:"))
    {
        let len = parse_u64(line, len)?;
        let len = usize::try_from(len)
            .map_err(|_| IlError::line(line, format!("data length `{token}` is out of range")))?;
        return Ok(vec![0; len]);
    }
    if let Some(hex) = token.strip_prefix("hex:") {
        return parse_hex_bytes(line, hex);
    }
    parse_string(line, token).map(String::into_bytes)
}

fn parse_hex_bytes(line: usize, hex: &str) -> Result<Vec<u8>> {
    let hex = hex.replace('_', "");
    if hex.len() % 2 != 0 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(IlError::line(line, format!("invalid hex data `hex:{hex}`")));
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|_| IlError::line(line, format!("invalid hex data `hex:{hex}`")))
        })
        .collect()
}

pub(crate) fn parse_address_literal(line: usize, token: &str) -> Result<AddressExpr> {
    if let Some(index) = parse_account_index_token(token) {
        if index == 0 {
            return Err(IlError::line(
                line,
                "account:0 is reserved for the implicit harness account",
            ));
        }
        return Ok(AddressExpr::AccountKey(index));
    }
    match token {
        "system" => Ok(AddressExpr::Static(PubkeyBytes::SYSTEM)),
        "harness" => Ok(AddressExpr::Static(PubkeyBytes(harness_program_id_bytes()))),
        "program" => Ok(AddressExpr::ProgramId),
        "sysvar:clock" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::clock::id().to_bytes(),
        ))),
        "sysvar:rent" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::rent::id().to_bytes(),
        ))),
        "sysvar:recent_blockhashes" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::recent_blockhashes::id().to_bytes(),
        ))),
        "sysvar:epoch_schedule" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::epoch_schedule::id().to_bytes(),
        ))),
        "sysvar:epoch_rewards" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::epoch_rewards::id().to_bytes(),
        ))),
        "sysvar:last_restart_slot" => Ok(AddressExpr::Static(PubkeyBytes(
            sysvar::last_restart_slot::id().to_bytes(),
        ))),
        _ => parse_hex_pubkey(token)
            .or_else(|| parse_base58_pubkey(token))
            .map(|pubkey| AddressExpr::Static(PubkeyBytes(pubkey)))
            .ok_or_else(|| IlError::line(line, format!("invalid address literal `{token}`"))),
    }
}

pub(crate) fn parse_account_index_token(token: &str) -> Option<usize> {
    token.strip_prefix("account:")?.parse().ok()
}

fn parse_account_literal(line: usize, token: &str) -> Result<usize> {
    let index = parse_account_index_token(token).ok_or_else(|| {
        IlError::line(
            line,
            format!("invalid account literal `{token}`, expected `account:N`"),
        )
    })?;
    if index == 0 {
        return Err(IlError::line(
            line,
            "account:0 is reserved for the implicit harness account",
        ));
    }
    Ok(index)
}

fn parse_hex_pubkey(token: &str) -> Option<[u8; 32]> {
    let hex = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
        .unwrap_or(token);
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0u8; 32];
    for index in 0usize..32 {
        let start = index.checked_mul(2)?;
        let end = start.checked_add(2)?;
        bytes[index] = u8::from_str_radix(&hex[start..end], 16).ok()?;
    }
    Some(bytes)
}

fn parse_base58_pubkey(token: &str) -> Option<[u8; 32]> {
    Address::from_str(token)
        .ok()
        .map(|address| address.to_bytes())
}
