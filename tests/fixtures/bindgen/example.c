#include "example.h"
#include <string.h>

int add_i32(int a, int b) {
    return a + b;
}

int returns_42(void) {
    return 42;
}

size_t strlen_like(const void *s) {
    return strlen((const char *)s);
}
