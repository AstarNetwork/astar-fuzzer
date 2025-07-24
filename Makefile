# Makefile
.PHONY: fuzz fuzz-parallel build triage clean

# Default fuzzing (single job)
fuzz:
	SKIP_WASM_BUILD=1 cargo ziggy fuzz

# Parallel fuzzing (20 jobs, 30s timeout)
fuzz-parallel:
	SKIP_WASM_BUILD=1 cargo ziggy fuzz -t 30 -j 20 --no-honggfuzz

# Build fuzzer (only needed when code changes)
build:
	SKIP_WASM_BUILD=1 cargo ziggy build

# Analyze crashes
triage:
	SKIP_WASM_BUILD=1 cargo ziggy triage

# build html analysis output
plot:
	SKIP_WASM_BUILD=1 cargo ziggy plot