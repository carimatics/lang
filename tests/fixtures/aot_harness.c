#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Preserve two's-complement bits without signed C overflow or an out-of-range
// unsigned-to-signed conversion. These fixtures target int64_t tier-1 hosts.
static int64_t signed_bits(uint64_t bits) {
    int64_t value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}

static int64_t wrapping_add(int64_t left, int64_t right) {
    return signed_bits((uint64_t)left + (uint64_t)right);
}

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_fibonacci(int64_t value);
extern int64_t prototype_host_allocation_burst(int64_t value);
extern int64_t prototype_host_allocation_churn(int64_t value);
extern int64_t prototype_host_allocation(int64_t value);
extern int64_t prototype_host_allocation_variable(int64_t value);
extern int64_t prototype_host_callback(int64_t value);
extern int64_t prototype_min(int64_t left, int64_t right);
extern int64_t prototype_sum_below(int64_t value, int64_t cutoff);
extern int64_t prototype_sum_to(int64_t value);
extern int64_t run(int64_t value);
extern uint32_t component_transfer_owned_resource(uint32_t value);

int64_t lang_host_i64_0(int64_t value) {
    return wrapping_add(value, value);
}

int64_t lang_host_i64_1(int64_t value) {
    int64_t *allocation = malloc(sizeof(*allocation));
    if (allocation == NULL) {
        abort();
    }
    *allocation = value;
    int64_t result = *allocation;
    free(allocation);
    return result;
}

int64_t lang_host_i64_2(int64_t value) {
    int64_t *allocations[64];
    for (int64_t index = 0; index < 64; index++) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = wrapping_add(value, index);
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 64; index++) {
        result = wrapping_add(result, *allocations[index]);
        free(allocations[index]);
    }
    return result;
}

int64_t lang_host_i64_3(int64_t value) {
    int64_t *allocations[64];
    for (int64_t index = 0; index < 64; index++) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = wrapping_add(value, index);
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 64; index += 2) {
        result = wrapping_add(result, *allocations[index]);
        free(allocations[index]);
    }
    for (int64_t index = 0; index < 64; index += 2) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = wrapping_add(value, 64 + index);
    }
    for (int64_t index = 0; index < 64; index++) {
        result = wrapping_add(result, *allocations[index]);
        free(allocations[index]);
    }
    return result;
}

int64_t lang_host_i64_4(int64_t value) {
    int64_t *allocations[16];
    for (int64_t index = 0; index < 16; index++) {
        allocations[index] = malloc((size_t)(index + 1) * sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        allocations[index][0] = wrapping_add(value, index);
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 16; index++) {
        result = wrapping_add(result, allocations[index][0]);
        free(allocations[index]);
    }
    return result;
}

int64_t lang_host_i64_5(int64_t value) {
    return wrapping_add(value, value);
}

int main(void) {
    const int64_t edges[] = { INT64_MIN, INT64_MAX, -1, 0, 1 };
    for (size_t index = 0; index < sizeof(edges) / sizeof(edges[0]); index++) {
        int64_t value = edges[index];
        uint64_t bits = (uint64_t)value;
        if (prototype_add(value, 1) != signed_bits(bits + 1)
            || prototype_host_callback(value) != signed_bits(bits * 2)
            || run(value) != signed_bits(bits * 2)
            || prototype_host_allocation(value) != value
            || prototype_host_allocation_burst(value) != signed_bits(bits * 64 + 2016)
            || prototype_host_allocation_churn(value) != signed_bits(bits * 96 + 5056)
            || prototype_host_allocation_variable(value) != signed_bits(bits * 16 + 120)) {
            return 1;
        }
    }
    if (prototype_min(INT64_MIN, INT64_MAX) != INT64_MIN
        || prototype_fibonacci(93) != INT64_C(-6246583658587674878)) {
        return 1;
    }
    int64_t add = prototype_add(20, 22);
    int64_t fibonacci = prototype_fibonacci(20);
    int64_t allocation = prototype_host_allocation(-9);
    int64_t allocation_burst = prototype_host_allocation_burst(-9);
    int64_t allocation_churn = prototype_host_allocation_churn(-9);
    int64_t allocation_variable = prototype_host_allocation_variable(-9);
    int64_t callback = prototype_host_callback(21);
    int64_t minimum = prototype_min(-7, 4);
    int64_t sum_below = prototype_sum_below(64, 33);
    int64_t sum = prototype_sum_to(64);
    int64_t component = run(21);
    uint32_t resource = component_transfer_owned_resource(1);
    printf("%lld %lld %lld %lld %lld %lld %lld %lld %lld %lld %lld %u\n", (long long)add, (long long)fibonacci, (long long)allocation, (long long)allocation_burst, (long long)allocation_churn, (long long)allocation_variable, (long long)callback, (long long)minimum, (long long)sum_below, (long long)sum, (long long)component, resource);
    return add == 42 && fibonacci == 6765 && allocation == -9 && allocation_burst == 1440 && allocation_churn == 4192 && allocation_variable == -24 && callback == 42 && minimum == -7 && sum_below == 528 && sum == 2080 && component == 42 && resource == 1 ? 0 : 1;
}
