// Halo 侧 C 门面：把 Status Trio 的 STPowerUIChargeBridge（Objective-C）
// 折成固定布局的结构体与整数返回，Rust 只认这层 ABI。
#import "PowerUIPrivateAPI.h"

#include <stddef.h>
#include <string.h>

typedef struct {
    int available;
    int reason;
    int valid;
    unsigned long long mcl_enabled_raw;
    long long mcl_limit;
    int override_allowed;
    char error[512];
} HaloPowerUIStateC;

static void HaloCopyError(char *dst, size_t len, NSString *message) {
    if (dst == NULL || len == 0) return;
    dst[0] = '\0';
    if (message == nil) return;
    const char *utf8 = message.UTF8String;
    if (utf8 == NULL) return;
    strncpy(dst, utf8, len - 1);
    dst[len - 1] = '\0';
}

void halo_powerui_read(HaloPowerUIStateC *out) {
    if (out == NULL) return;
    memset(out, 0, sizeof(*out));
    out->mcl_limit = -1;
    @autoreleasepool {
        STPowerUIChargeBridge *bridge = [STPowerUIChargeBridge new];
        STPowerUIChargeCapability *capability = [bridge capability];
        out->available = capability.isAvailable ? 1 : 0;
        out->reason = (int)capability.reason;
        if (!out->available) {
            HaloCopyError(out->error, sizeof(out->error), @"PowerUI 不可用");
            return;
        }
        STPowerUIChargeState *state = [bridge readState];
        out->valid = state.hasValidState ? 1 : 0;
        out->mcl_enabled_raw = (unsigned long long)state.mclEnabledRawValue;
        out->mcl_limit = (long long)state.mclLimit;
        out->override_allowed = state.chargingOverrideAllowed ? 1 : 0;
        HaloCopyError(out->error, sizeof(out->error), state.errorMessage);
    }
}

int halo_powerui_charge_to_full(char *err, size_t len) {
    if (err != NULL && len > 0) err[0] = '\0';
    @autoreleasepool {
        STPowerUIChargeWriteResult *result =
            [[STPowerUIChargeBridge new] temporarilyDisableMCL];
        if (!result.didSucceed) {
            HaloCopyError(err, len, result.errorMessage);
        }
        return result.didSucceed ? 1 : 0;
    }
}

int halo_powerui_restore_limit(int limit, char *err, size_t len) {
    if (err != NULL && len > 0) err[0] = '\0';
    @autoreleasepool {
        STPowerUIChargeWriteResult *result =
            [[STPowerUIChargeBridge new] restoreChargeLimit:(NSInteger)limit];
        if (!result.didSucceed) {
            HaloCopyError(err, len, result.errorMessage);
        }
        return result.didSucceed ? 1 : 0;
    }
}
