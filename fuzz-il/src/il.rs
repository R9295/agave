use {
    pest::{Parser, iterators::Pair},
    pest_derive::Parser,
    solana_address::Address,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PubkeyBytes(pub(crate) [u8; 32]);

impl PubkeyBytes {
    pub(crate) const SYSTEM: Self = Self([0; 32]);
    const HARNESS: Self = Self([
        0xa1, 0xb2, 0xc3, 0xd4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0xde, 0xad, 0xbe, 0xef,
    ]);
    const SYSVAR_CLOCK: Self = Self([
        6, 167, 213, 23, 24, 199, 116, 201, 40, 86, 99, 152, 105, 29, 94, 182, 139, 94, 184, 163,
        155, 75, 109, 92, 115, 85, 91, 33, 0, 0, 0, 0,
    ]);
    const SYSVAR_RENT: Self = Self([
        6, 167, 213, 23, 25, 47, 10, 175, 198, 242, 101, 227, 251, 119, 204, 122, 218, 130, 197,
        41, 208, 190, 59, 19, 110, 45, 0, 85, 32, 0, 0, 0,
    ]);
    const SYSVAR_RECENT_BLOCKHASHES: Self = Self([
        6, 167, 213, 23, 25, 44, 86, 142, 224, 138, 132, 95, 115, 210, 151, 136, 207, 3, 92, 49,
        69, 178, 26, 179, 68, 216, 6, 46, 169, 64, 0, 0,
    ]);
    const SYSVAR_EPOCH_SCHEDULE: Self = Self([
        6, 167, 213, 23, 24, 220, 63, 238, 2, 211, 64, 70, 47, 247, 80, 215, 227, 84, 11, 26, 215,
        23, 158, 192, 12, 100, 110, 175, 64, 0, 0, 0,
    ]);
    const SYSVAR_EPOCH_REWARDS: Self = Self([
        6, 167, 213, 23, 24, 219, 192, 4, 178, 82, 211, 122, 242, 80, 71, 138, 167, 246, 234, 92,
        144, 27, 245, 23, 31, 173, 4, 25, 16, 0, 0, 0,
    ]);
    const SYSVAR_LAST_RESTART_SLOT: Self = Self([
        6, 167, 213, 23, 24, 138, 113, 244, 87, 27, 95, 209, 168, 250, 245, 196, 217, 219, 152,
        247, 19, 6, 33, 22, 86, 68, 100, 18, 88, 0, 0, 0,
    ]);

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

impl AddressExpr {
    pub(crate) fn static_or_default(&self) -> PubkeyBytes {
        match self {
            Self::Static(pubkey) => *pubkey,
            Self::AccountKey(_) | Self::ProgramId => PubkeyBytes::SYSTEM,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Value {
    U8(u8),
    U64(u64),
    String(String),
    Address(AddressExpr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueKind {
    U8,
    U64,
    String,
    Address,
}

impl Value {
    pub(crate) fn kind(&self) -> ValueKind {
        match self {
            Self::U8(_) => ValueKind::U8,
            Self::U64(_) => ValueKind::U64,
            Self::String(_) => ValueKind::String,
            Self::Address(_) => ValueKind::Address,
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
    Invoke {
        line: usize,
        op: String,
        args: Vec<String>,
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
        _ => None,
    }
}

fn collect_statements(pair: Pair<'_, Rule>, program: &mut Program) -> Result<()> {
    match pair.as_rule() {
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
    let args = inner
        .next()
        .map(parse_arg_list)
        .transpose()?
        .unwrap_or_default();
    Ok(Statement::Invoke { line, op, args })
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

pub(crate) fn parse_address_literal(line: usize, token: &str) -> Result<AddressExpr> {
    if let Some(index) = parse_account_index_token(token) {
        return Ok(AddressExpr::AccountKey(index));
    }
    match token {
        "system" => Ok(AddressExpr::Static(PubkeyBytes::SYSTEM)),
        "harness" => Ok(AddressExpr::Static(PubkeyBytes::HARNESS)),
        "program" => Ok(AddressExpr::ProgramId),
        "sysvar:clock" => Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_CLOCK)),
        "sysvar:rent" => Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_RENT)),
        "sysvar:recent_blockhashes" => {
            Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_RECENT_BLOCKHASHES))
        }
        "sysvar:epoch_schedule" => Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_EPOCH_SCHEDULE)),
        "sysvar:epoch_rewards" => Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_EPOCH_REWARDS)),
        "sysvar:last_restart_slot" => {
            Ok(AddressExpr::Static(PubkeyBytes::SYSVAR_LAST_RESTART_SLOT))
        }
        _ => parse_hex_pubkey(token)
            .or_else(|| parse_base58_pubkey(token))
            .map(|pubkey| AddressExpr::Static(PubkeyBytes(pubkey)))
            .ok_or_else(|| IlError::line(line, format!("invalid address literal `{token}`"))),
    }
}

pub(crate) fn parse_account_index_token(token: &str) -> Option<usize> {
    token.strip_prefix("account:")?.parse().ok()
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
