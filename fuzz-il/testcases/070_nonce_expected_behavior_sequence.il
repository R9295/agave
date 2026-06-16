# source: system_instruction.rs expected_behavior
# initialize, advance twice, then withdraw the nonce balance
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
AdvanceNonceAccount | ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (account:1, false, true)
AdvanceNonceAccount | ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (account:1, false, true)
LoadU64 withdraw_lamports = 1000042
WithdrawNonceAccount | withdraw_lamports ; (account:0, true, false), (account:1, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false), (account:2, false, true)
