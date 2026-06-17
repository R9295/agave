/// Solana SDK header bundle: typedefs + syscall prototypes + a generated
/// `entrypoint` that walks the SBPF input region into `SolParameters`. The
/// `assemble` step splices the per-program `user_body` into this just before
/// `entrypoint`, and the body call just before `entrypoint`'s `return 0;`.
pub const TEMPLATE: &str = "
/* Types */
typedef signed char int8_t;
typedef unsigned char uint8_t;
typedef signed short int16_t;
typedef unsigned short uint16_t;
typedef signed int int32_t;
typedef unsigned int uint32_t;
typedef signed long int int64_t;
typedef unsigned long int uint64_t;
typedef int64_t ssize_t;
typedef uint64_t size_t;
typedef struct {
  const uint8_t *addr;
  uint64_t len;
} SolBytes;
typedef struct {
  uint8_t x[32];
} SolPubkey;

/* Syscalls */
void sol_memcpy_(void *, const void *, uint64_t);
void sol_memmove_(void *, const void *, uint64_t);
void sol_memset_(void *, uint8_t, uint64_t);
void sol_memcmp_(const void *, const void *, uint64_t, int32_t *);
void sol_set_return_data(const uint8_t *, uint64_t);
uint64_t sol_get_return_data(uint8_t *, uint64_t, SolPubkey *);
uint64_t sol_remaining_compute_units(void);
void sol_get_clock_sysvar(uint8_t *);
void sol_get_epoch_schedule_sysvar(uint8_t *);
void sol_get_rent_sysvar(uint8_t *);
void sol_get_last_restart_slot(uint8_t *);
void sol_get_epoch_rewards_sysvar(uint8_t *);
uint64_t sol_get_sysvar(const uint8_t *, uint8_t *, uint64_t, uint64_t);
uint64_t sol_get_stack_height(void);
uint64_t sol_get_epoch_stake(const uint8_t *);
uint64_t sol_get_processed_sibling_instruction(uint64_t, void *, uint8_t *, uint8_t *, uint8_t *);

void *memcpy(void *dst, const void *src, size_t n) {
    sol_memcpy_(dst, src, n);
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    sol_memmove_(dst, src, n);
    return dst;
}

void *memset(void *dst, int value, size_t n) {
    sol_memset_(dst, (uint8_t)value, n);
    return dst;
}

int memcmp(const void *left, const void *right, size_t n) {
    int32_t result = 0;
    sol_memcmp_(left, right, n, &result);
    return result;
}

/* Account Types */
typedef struct {
  const uint8_t *addr;
  uint64_t len;
} SolSignerSeed;

typedef struct {
  const SolSignerSeed *addr;
  uint64_t len;
} SolSignerSeeds;

uint64_t sol_create_program_address(const SolSignerSeed *, int, const SolPubkey *, SolPubkey *);
uint64_t sol_try_find_program_address(const SolSignerSeed *, int, const SolPubkey *, SolPubkey *, \
                            uint8_t *);

typedef struct {
  SolPubkey *key;
  uint64_t *lamports;
  uint64_t data_len;
  uint8_t *data;
  SolPubkey *owner;
  uint64_t rent_epoch;
  _Bool is_signer;
  _Bool is_writable;
  _Bool executable;
} SolAccountInfo;

typedef struct {
  SolAccountInfo* ka;
  uint64_t ka_num;
  const uint8_t *data;
  uint64_t data_len;
  const SolPubkey *program_id;
} SolParameters;

uint64_t entrypoint(const uint8_t *input);

typedef struct {
  SolPubkey *pubkey;
  _Bool is_writable;
  _Bool is_signer;
} SolAccountMeta;

typedef struct {
  SolPubkey *program_id;
  SolAccountMeta *accounts;
  uint64_t account_len;
  uint8_t *data;
  uint64_t data_len;
} SolInstruction;

uint64_t sol_invoke_signed_c(
  const SolInstruction *,
  const SolAccountInfo *,
  int,
  const SolSignerSeeds *,
  int
);

/* Pubkey constants used by generated code. Defined once in the template */
static const SolPubkey SYSTEM_PROGRAM_ID = { .x = {0} };

#define MAX_ACCOUNT_GROW_LIMIT (10 * 1024)

/* Helper Functions */
static uint64_t align_up_8(uint64_t x) {
    return (x + 7) & ~((uint64_t)7);
}

static uint64_t original_account_data_len(const SolAccountInfo *account) {
    return *(uint32_t *)((uint8_t *)account->key - sizeof(uint32_t));
}

static void account_resize(SolAccountInfo *account, uint64_t new_len) {
    if (account == 0 || account->key == 0 || account->data == 0) {
        return;
    }
    uint64_t original_len = original_account_data_len(account);
    if (new_len > original_len + MAX_ACCOUNT_GROW_LIMIT) {
        return;
    }
    account->data_len = new_len;
    *(uint64_t *)(account->data - sizeof(uint64_t)) = new_len;
}

static void write_account_data(
    SolAccountInfo *account,
    uint64_t offset,
    uint64_t len,
    uint8_t value
) {
    memset(account->data + offset, value, len);
}

static void set_account_owner(SolAccountInfo *account, const SolPubkey *owner) {
    sol_memcpy_(account->owner->x, owner->x, 32);
}

static void add_account_lamports(SolAccountInfo *account, uint64_t amount) {
    *account->lamports += amount;
}

static void sub_account_lamports(SolAccountInfo *account, uint64_t amount) {
    *account->lamports -= amount;
}

static _Bool custom_accounts_deserialize(
    const uint8_t *input,
    SolParameters *params,
    uint64_t ka_capacity
) {
    if (input == 0 || params == 0) {
        return 0;
    }
    uint8_t *cur = (uint8_t *)input;
    params->ka_num = *(uint64_t *)cur;
    cur += sizeof(uint64_t);
    if (params->ka_num > ka_capacity) {
        return 0;
    }
    for (uint64_t i = 0; i < params->ka_num; i++) {
        uint8_t dup = *cur;
        cur += sizeof(uint8_t);
        if (dup != 0xFF) {
            params->ka[i] = params->ka[dup];
            cur += 7;
            continue;
        }
        SolAccountInfo *ai = &params->ka[i];
        ai->is_signer = (*cur++ != 0);
        ai->is_writable = (*cur++ != 0);
        ai->executable = (*cur++ != 0);
        uint32_t *original_data_len = (uint32_t *)cur;
        cur += sizeof(uint32_t);
        ai->key = (SolPubkey *)cur;
        cur += sizeof(SolPubkey);
        ai->owner = (SolPubkey *)cur;
        cur += sizeof(SolPubkey);
        ai->lamports = (uint64_t *)cur;
        cur += sizeof(uint64_t);
        ai->data_len = *(uint64_t *)cur;
        cur += sizeof(uint64_t);
        *original_data_len = (uint32_t)ai->data_len;
        ai->data = cur;
        cur += ai->data_len;
        cur += MAX_ACCOUNT_GROW_LIMIT;
        cur = (uint8_t *)align_up_8((uint64_t)cur);
        ai->rent_epoch = *(uint64_t *)cur;
        cur += sizeof(uint64_t);
    }
    params->data_len = *(uint64_t *)cur;
    cur += sizeof(uint64_t);
    params->data = cur;
    cur += params->data_len;
    params->program_id = (SolPubkey *)cur;
    return 1;
}
/* Entrypoint */
extern uint64_t entrypoint(const uint8_t *input) {
    SolAccountInfo accounts[16];
    SolParameters params = (SolParameters){.ka = accounts};
    if (!custom_accounts_deserialize(input, &params, (sizeof(accounts) / sizeof(accounts[0])))) {
        return ((uint64_t)(2) << 32);
    }
    return 0;
}";
