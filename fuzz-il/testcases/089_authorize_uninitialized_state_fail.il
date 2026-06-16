# source: system_instruction.rs authorize_inx_uninitialized_state_fail
# nonce account state is uninitialized
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ;
  (account:0, true, false),
  (account:1, false, true)
