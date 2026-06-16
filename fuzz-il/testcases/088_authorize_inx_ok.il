# source: system_instruction.rs authorize_inx_ok
# nonce account is initialized and current authority signs
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ; (account:0, true, false), (account:1, false, true)
