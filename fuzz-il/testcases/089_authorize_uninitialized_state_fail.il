# source: system_instruction.rs authorize_inx_uninitialized_state_fail
# nonce account state is uninitialized
LoadAccountState account:0 NonceUninitialized
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:0, true, false),
  (account:1, false, true)
