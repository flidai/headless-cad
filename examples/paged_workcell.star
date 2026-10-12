# Metadata residency workload: repeated tooling cells, not a modeling benchmark.
# Default: 400 occurrences. --param stations=1000 gives 100,000 occurrences;
# build that configuration with --display-only.
stations = param(name = "stations", default = 4)
def check_count():
    if stations < 1 or stations > 1000 or stations != int(stations):
        fail("stations must be an integer in 1..1000")
check_count()
profile = rectangle(id = "block_profile", width = 8, depth = 8)
body = extrude(id = "block", profile = profile, height = 6)
block = part(id = "tool", body = body)
cell = assembly(id = "cell", instances = [
    instance(id = "tool_" + str(i), part = block,
             offset = [(i % 10)*12, int(i / 10)*12, 0])
    for i in range(100)
])
machine = assembly(id = "machine", instances = [
    assembly_instance(id = "station_" + str(i), assembly = cell,
                      offset = [(i % 40)*140, int(i / 40)*140, 0])
    for i in range(int(stations))
])
assembly_output(name = "paged_workcell", assembly = machine)
