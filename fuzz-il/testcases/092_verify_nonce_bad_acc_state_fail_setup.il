# source: system_instruction.rs verify_nonce_bad_acc_state_fail
# account remains uninitialized; representative instruction probes the state
AdvanceNonceAccount | ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (account:1, false, true)
