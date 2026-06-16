# source: system_instruction.rs default_is_uninitialized
# harness explicitly provides account:1 with uninitialized nonce state
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceUninitialized
UpgradeNonceAccount | ;
  (account:1, true, false)
