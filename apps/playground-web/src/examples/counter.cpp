#include <cstdint>

// MiniJAM Stage 0 Counter, expressed as restricted C++.
namespace counter {
struct State { std::uint64_t value; };
}

extern "C" void refine() {}
extern "C" void accumulate() {}
