# source: system_instruction.rs authorize_inx_ok
# nonce account is initialized and current authority signs
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:2 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:1, true, false),
  (account:2, false, true)
