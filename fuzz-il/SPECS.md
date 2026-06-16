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

Lowering:
- Parse IL with pest into load, account-state, and invoke statements.
- Loads populate named values and typed implicit queues.
- Invokes require an explicit `;` account-meta list.
- Operands resolve from pipe args first, else typed queues.
- System ops lower to bincode `SystemInstruction` data.
- Dynamic account/program addresses are emitted as patches.
- Account-state declarations are carried to `InstrContext`; they emit no CPI code.
- C rendering patches data, builds metas, then CPIs to system.

Accounts:
- Account metas are `(pubkey, writable, signer)` tuples.
- Pubkeys resolve from names, `account:N`, `system`, sysvars, or literals.
- `account:N` maps to a deterministic synthetic key for protosol.
- `LoadAccountState <target> <kind> [args...]` sets initial state.
- Testcases declare every referenced `account:N` and sysvar target.
- Duplicate instruction metas are preserved in order.
- Account states are deduplicated by pubkey for `InstrContext.accounts`.
- Missing or duplicate account-state declarations are errors.
- System/sysvar accounts get concrete owner/data needed by the harness.

Account State Presets:
| IL kind | Lamports | Data | Owner |
| - | - | - | - |
| `SystemFunded`/`SystemEmpty`/`SystemAllocated` | arg/0/0 | none/none/supplied bytes | system |
| `NonceInitialized`/`NonceInitializedLowRent` | rent min + extra / rent min - 1 | serialized initialized nonce | system |
| `NonceUninitialized` | rent min | serialized uninitialized nonce padded to nonce size | system |
| `SysvarRent`/`SysvarRecentBlockhashes`/`SysvarRecentBlockhashesEmpty` | 1 | serialized sysvar data | sysvar |
| `ForeignEmpty`/`ForeignData` | 0 / arg | none / supplied bytes | explicit owner arg |

Account State Syntax:
- Targets are `account:N` or fixed addresses like `sysvar:rent`.
- Data args accept `zeros:N`, `hex:...`, or quoted/raw string bytes.
- `SystemFunded` takes lamports; `SystemAllocated` takes data.
- `NonceInitialized authority extra_lamports`.
- `NonceInitializedLowRent authority`.
- `ForeignEmpty owner`; `ForeignData lamports data owner`.
- Later declarations for the same target are rejected.

InstrContext:
- One protosol `InstrContext` is built per lowered invocation.
- `program_id` is the system program id.
- `instr_accounts` preserves lowered meta order and flags.
- `accounts` contains resolved metas with declared account state.
- Missing state for any resolved meta is rejected.
- `data` is the lowered instruction data after all address patches.
- `cu_avail` is 1_400_000 and `features` is unset.
- The context is printed, then protobuf-encoded to a temp `.instr.pb`.
- The protobuf path is printed after the textual context dump.
