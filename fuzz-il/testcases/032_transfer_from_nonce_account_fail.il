# source: test_transfer_lamports_from_nonce_account_fail
# account:0 is an initialized nonce account
LoadU64 lamports = 50
Transfer | lamports ; (account:0, true, true), (account:1, true, false)
