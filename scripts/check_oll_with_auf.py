import cfop_solver

cfop_solver.init_tables()

from cfop_solver import CubieCube, apply_alg_str, invert_alg, convert_alg_to_basic, OLL_ALGS

solved = CubieCube.solved()

print("Generating all OLL patterns with AUF variations:")
print("="*70)

all_patterns_with_auf = {}

# for case_num in sorted(OLL_ALGS.keys()):
for case_num in sorted([18, ]):
    alg = OLL_ALGS[case_num]
    alg = "y' M' U"
    print(alg)
    basic_alg = convert_alg_to_basic(alg)
    print(basic_alg)
    exit()
    inv = invert_alg(basic_alg)
    s = apply_alg_str(solved.copy(), inv)
    
    # Generate all 4 AUF variations
    for auf in range(4):
        rotated = s.apply_u_rot(auf)
        co = tuple(rotated.co[i] for i in range(4))
        eo = tuple(rotated.eo[i] for i in range(4))
        pattern = (co, eo)
        print(f"Case {case_num:2d} with AUF={auf}: co={co}, eo={eo}")
        if pattern not in all_patterns_with_auf:
            all_patterns_with_auf[pattern] = []
        all_patterns_with_auf[pattern].append((case_num, auf))

print(f"Total unique (CO,EO) patterns with AUF: {len(all_patterns_with_auf)}")

# Check target
target = ((2, 0, 0, 1), (1, 1, 0, 0))
print(f"\nTarget pattern {target}:")
if target in all_patterns_with_auf:
    matches = all_patterns_with_auf[target]
    print(f"  FOUND! Matches: {matches}")
    for case_num, auf in matches:
        print(f"    Case {case_num} with AUF={auf}")
else:
    print(f"  NOT FOUND")

# Also check some nearby patterns
print("\nChecking nearby patterns:")
test_patterns = [
    ((2, 0, 0, 1), (1, 1, 0, 0)),
    ((1, 2, 0, 0), (0, 1, 1, 0)),
    ((0, 1, 2, 0), (0, 0, 1, 1)),
    ((0, 0, 1, 2), (1, 0, 0, 1)),
]

for p in test_patterns:
    if p in all_patterns_with_auf:
        print(f"  {p}: {all_patterns_with_auf[p]}")
    else:
        print(f"  {p}: NOT FOUND")
