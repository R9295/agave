# source: programs/system/benches/system.rs prep_allocate
LoadAccountState account:0 SystemEmpty
LoadU64 space = 2
Allocate | space ;
  (account:0, true, true)
