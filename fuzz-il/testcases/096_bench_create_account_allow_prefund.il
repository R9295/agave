# source: programs/system/benches/system.rs bench_create_account_allow_prefund
LoadU64 lamports = 1
LoadU64 space = 2
LoadAddress owner = system
CreateAccountAllowPrefund | lamports, space, owner ; (account:0, true, true), (account:1, true, true)
