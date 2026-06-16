# source: test_process_authorize_bad_account_data_fail
LoadAddress authority = account:0
AuthorizeNonceAccount | authority ;
  (account:0, true, false),
  (account:1, false, true)
