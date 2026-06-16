# source: test_process_authorize_bad_account_data_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress authority = system
AuthorizeNonceAccount | authority ;
  (account:1, true, false),
  (account:2, false, true)
