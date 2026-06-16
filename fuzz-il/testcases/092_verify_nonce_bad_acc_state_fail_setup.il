# source: system_instruction.rs verify_nonce_bad_acc_state_fail
# account remains uninitialized; representative instruction probes the state
LoadAccountState account:0 NonceUninitialized
LoadAccountState account:1 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
