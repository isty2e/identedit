#include <string>

class Processor {
public:
    int value;

    int helper() const {
        return value + 1;
    }
};

int process_data(int value) {
    return value + 1;
}
