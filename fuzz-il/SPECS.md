Your goal is to create a Fuzz-IL like implementation for Solana.
The goal is to parse the IL, lower, compile using compile.rs to generate an elf.
Append the the lowered code to the entrypoint in TEMPLATE in template.rs.

You should implement the following fields in the IL.
LoadU8
LoadAddress
LoadU64
LoadString
LoadAccount
Account metas are explicit tuples: (pubkey, writable, signer)
CreateAccount | lamports, space, owner ;
  (from, true, true),
  (to, true, true)
Assign | owner ;
  (account, true, true)
Transfer | lamports ;
  (from, true, true),
  (to, true, false)
CreateAccountWithSeed | base, seed, lamports, space, owner ;
  (from, true, true),
  (to, true, false),
  (base_authority, false, true)
AdvanceNonceAccount | ;
  (nonce, true, false),
  (sysvar:recent_blockhashes, false, false),
  (authority, false, true)
WithdrawNonceAccount | lamports ;
  (nonce, true, false),
  (to, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (authority, false, true)
InitializeNonceAccount | address ;
  (nonce, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
AuthorizeNonceAccount | address ;
  (nonce, true, false),
  (authority, false, true)
Allocate | space ;
  (account, true, true)
AllocateWithSeed | base, seed, space, owner ;
  (account, true, false),
  (base_authority, false, true)
AssignWithSeed | base, seed, owner ;
  (account, true, false),
  (base_authority, false, true)
TransferWithSeed | lamports, seed, owner ;
  (from, true, false),
  (base, false, true),
  (to, true, false)
UpgradeNonceAccount | ;
  (nonce, true, false)
CreateAccountAllowPrefund | lamports, space, owner ;
  (to, true, true),
  (from, true, true)
AccountResize | new_len ;
  (account, true, false)
WriteAccountData | offset, len, value ;
  (account, true, false)

Lowering:
- Parse IL with pest into load, account-state, and invoke statements.
- Loads populate named values and typed implicit queues.
- Invokes require an explicit `;` account-meta list.
- Operands resolve from pipe args first, else typed queues.
- System ops lower to bincode `SystemInstruction` data.
- Dynamic account/program addresses are emitted as patches.
- Account-state declarations are carried to `InstrContext`; they emit no CPI code.
- Harness ops emit no CPI: `AccountResize` mutates data_len; `WriteAccountData` calls `memset`.
- Resize over-limit grows no-op; writes are attempted without account data_len precheck.
- C rendering patches data, builds metas, then CPIs to system.

Accounts:
- Account metas are `(pubkey, writable, signer)` tuples.
- Pubkeys resolve from names, `account:N`, `system`, sysvars, or literals.
- `account:0` is reserved for the implicit harness; IL starts at `account:1`.
- `account:N` maps to a deterministic synthetic key for protosol caller slots.
- `LoadAccountState <target> | <data> | <owner> | <lamports>` sets normal account state.
- `LoadAccountState <sysvar-address> <Sysvar*>` sets canonical sysvar state.
- Testcases declare every `account:N > 0`, required harness sysvars, and referenced sysvar target.
- Duplicate instruction metas are preserved in order.
- Account states are deduplicated by pubkey for `InstrContext.accounts`.
- Missing or duplicate account-state declarations are errors.
- The harness account is synthesized from the compiled ELF and is never declared.

Sysvar Presets:
| IL kind | Lamports | Data | Owner |
| - | - | - | - |
| `SysvarClock`/`SysvarRent`/`SysvarRecentBlockhashes`/`SysvarRecentBlockhashesEmpty` | 1 | serialized sysvar data | sysvar |

Account State Syntax:
- Targets are `account:N` or fixed addresses like `sysvar:clock`/`sysvar:rent`.
- Normal accounts use `LoadAccountState <target> | <data> | <owner> | <lamports>`.
- Data accepts `zeros:N`, `zero:N`, `hex:...`, or quoted/raw string bytes.
- Owner resolves from `system`, `harness`, `program`, sysvars, `account:N`, or literals.
- Lamports are explicit u64 values; the IL does not infer rent or funding.
- Only sysvars keep named presets; all other account bytes are explicit.
- Later declarations for the same target are rejected.

InstrContext:
- One protosol `InstrContext` is built for the compiled lowered program.
- `program_id` is the fixed harness program id.
- `accounts[0]` is the loader-v3 harness program account.
- A companion loader-v3 programdata account stores the ELF bytes.
- `instr_accounts[0]` passes the harness as caller slot `account:0`.
- User caller slots are dense `account:1..max` from declared states.
- The system program account is synthesized and appended for harness CPIs.
- Required `sysvar:clock`/`sysvar:rent` are in `accounts`; CPI literals append by meta.
- Outer `data` is empty; the ELF renders and CPIs system instructions.
- `cu_avail` is 1_400_000 and `features` is unset.
- The context is printed, then protobuf-encoded to a temp `.instr.pb`.
