# source: system_instruction.rs authorize_inx_bad_authority_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | hex:0100000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
LoadAccountState account:2 | zeros:0 | system | 1000000000
LoadAddress new_authority = account:2
AuthorizeNonceAccount | new_authority ;
  (account:1, true, false),
  (account:2, false, true)
