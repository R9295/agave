# source: programs/system/benches/system.rs prep_allocate
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemEmpty
LoadU64 space = 2
Allocate | space ;
  (account:1, true, true)
