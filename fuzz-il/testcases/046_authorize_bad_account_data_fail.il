# source: test_process_authorize_bad_account_data_fail
LoadAccountState account:0 SystemEmpty
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress authority = account:0
AuthorizeNonceAccount | authority ;
  (account:0, true, false),
  (account:1, false, true)
