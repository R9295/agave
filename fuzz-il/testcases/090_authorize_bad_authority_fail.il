# source: system_instruction.rs authorize_inx_bad_authority_fail
# nonce account authority is system, but account:1 signs instead
LoadAddress new_authority = system
AuthorizeNonceAccount | new_authority ; (account:0, true, false), (account:1, false, true)
