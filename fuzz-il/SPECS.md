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
CreateAccount | lamports, space, owner ; (from, true, true), (to, true, true)
Assign        | owner ; (account, true, true)
Transfer      | lamports ; (from, true, true), (to, true, false)
CreateAccountWithSeed | base, seed, lamports, space, owner ; (from, true, true), (to, true, false), (base_authority, false, true)
AdvanceNonceAccount   | ; (nonce, true, false), (sysvar:recent_blockhashes, false, false), (authority, false, true)
WithdrawNonceAccount  | lamports ; (nonce, true, false), (to, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false), (authority, false, true)
InitializeNonceAccount  | address ; (nonce, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
AuthorizeNonceAccount   | address ; (nonce, true, false), (authority, false, true)
Allocate                | space ; (account, true, true)
AllocateWithSeed        | base, seed, space, owner ; (account, true, false), (base_authority, false, true)
AssignWithSeed          | base, seed, owner ; (account, true, false), (base_authority, false, true)
TransferWithSeed        | lamports, seed, owner ; (from, true, false), (base, false, true), (to, true, false)
UpgradeNonceAccount     | ; (nonce, true, false)
CreateAccountAllowPrefund | lamports, space, owner ; (to, true, true), (from, true, true)
