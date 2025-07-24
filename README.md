# Astar Fuzzer

A fuzzing framework for the Astar runtime using structured input generation.

## Usage

```bash
# Compile the Docker
docker-compose build

# start container
docker-compose run fuzzer

# Inside container - Single job (easier for debug)
make fuzz

# Inside container - 20 parallel jobs
make fuzz-parallel
```

Once fuzzing reach a great coverage:
```bash
# Generate html analysis output
make plot

# Analyze crashes
make triage
```
