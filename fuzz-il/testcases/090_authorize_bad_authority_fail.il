# source: system_instruction.rs authorize_inx_bad_authority_fail
# nonce account authority is system, but account:2 signs instead
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized system 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:1, true, false),
  (account:2, false, true)
