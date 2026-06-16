# source: system_instruction.rs authorize_inx_bad_authority_fail
# nonce account authority is system, but account:1 signs instead
LoadAccountState account:0 NonceInitialized system 0
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:0, true, false),
  (account:1, false, true)
