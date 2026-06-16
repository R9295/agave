# source: system_instruction.rs authorize_inx_uninitialized_state_fail
# nonce account state is uninitialized
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:1, true, false),
  (account:2, false, true)
