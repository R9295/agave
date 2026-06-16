Your goal is to create a Fuzz-IL like implementation for Solana.
The goal is to parse the IL, lower, compile using compile.rs to generate an elf.
Append the the lowered code to the entrypoint in TEMPLATE in template.rs.

You should implement the following fields in the IL.
LoadU8
LoadAddress
LoadU64
LoadString
LoadAccount
CreateAccount | lamports, space, owner
Assign        | owner
Transfer      | lamports
CreateAccountWithSeed | base, seed, lamports, space, owner
AdvanceNonceAccount
WithdrawNonceAccount  | lamports
InitializeNonceAccount  | address
AuthorizeNonceAccount   | address
Allocate                | space
AllocateWithSeed        | base, seed, space, owner
AssignWithSeed          | base, seed, owner
TransferWithSeed        | lamports, from, to
UpgradeNonceAccount
CreateAccountAllowPrefund | lamports, space, owner
