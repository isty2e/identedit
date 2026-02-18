#include <stdio.h>

typedef struct {
    int value;
} Processor;

int process_data(int value) {
    return value + 1;
}

int helper(Processor processor) {
    return processor.value + 1;
}
