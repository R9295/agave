# source: system_instruction.rs authorize_inx_ok
# nonce account is initialized and current authority signs
LoadAccountState account:0 NonceInitialized account:1 0
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:0, true, false),
  (account:1, false, true)
