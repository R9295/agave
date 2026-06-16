# source: test_process_bogus_instruction
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadU64 lamports = 0
Transfer | lamports ;
  (account:1, false, true)
