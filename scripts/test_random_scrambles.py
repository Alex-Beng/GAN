import random
import time
import cProfile
import pstats
from io import StringIO
import cfop_solver
from cfop_solver import CFOPSolver, CubieCube, apply_alg_str

# cfop_solver.DEBUG = True

# WCA official moves for scramble generation
MOVES = ['U', "U'", 'U2', 'R', "R'", 'R2', 'F', "F'", 'F2', 
         'D', "D'", 'D2', 'L', "L'", 'L2', 'B', "B'", 'B2']

def generate_scramble(length=20):
    """Generate a random scramble"""
    scramble = []
    last_face = None
    
    for _ in range(length):
        # Avoid consecutive moves on same face or opposite faces
        while True:
            move = random.choice(MOVES)
            face = move[0]
            
            # Check if it's valid (different from last face and not opposite)
            if last_face is None:
                break
            
            opposite = {'U': 'D', 'D': 'U', 'R': 'L', 'L': 'R', 'F': 'B', 'B': 'F'}
            if face != last_face and face != opposite.get(last_face):
                break
        
        scramble.append(move)
        last_face = move[0]
    
    return ' '.join(scramble)


def test_single_scramble(solver, scramble):
    """Test a single scramble and return result"""
    state = CubieCube.solved()
    state = apply_alg_str(state, scramble)
    facelet = state.to_facelet()
    
    result = solver.solve(facelet)
    return result


def test_random_scrambles(num_tests=10, scramble_length=20, profile=False):
    """Test multiple random scrambles"""
    print(f"Testing {num_tests} random scrambles (length={scramble_length})...")
    print("=" * 70)
    
    solver = CFOPSolver()
    
    results = {
        'success': 0,
        'failed': 0,
        'alll_hit': 0,
        'alll_total': 0,
        'total_time': 0,
        'total_moves': 0,
        'min_time': float('inf'),
        'max_time': 0,
        'min_moves': float('inf'),
        'max_moves': 0,
        'failed_scrambles': [],
        'alll_miss_cases': [],
        'time_records': []
    }
    
    for i in range(num_tests):
        scramble = generate_scramble(scramble_length)
        print(f"\nTest {i+1}/{num_tests}")
        print(f"Scramble: {scramble}")
        alll_rate = results['alll_hit'] / results['alll_total'] * 100 if results['alll_total'] > 0 else 0
        print(f"ALLL hit rate: {alll_rate:.1f}%")
        
        start_time = time.time()
        
        if profile and i == 0:  # Profile the first one
            pr = cProfile.Profile()
            pr.enable()
            result = test_single_scramble(solver, scramble)
            pr.disable()
            
            # Print profiling results
            print("\n" + "=" * 70)
            print("PROFILING RESULTS (First scramble only):")
            print("=" * 70)
            s = StringIO()
            ps = pstats.Stats(pr, stream=s).sort_stats('cumulative')
            ps.print_stats(20)  # Top 20 functions
            print(s.getvalue())
            print("=" * 70 + "\n")
        else:
            result = test_single_scramble(solver, scramble)
        
        elapsed = time.time() - start_time
        
        if result is None:
            results['failed'] += 1
            results['failed_scrambles'].append(scramble)
            print(f"Status: FAILED")
        else:
            results['success'] += 1
            results['total_time'] += elapsed
            results['min_time'] = min(results['min_time'], elapsed)
            results['max_time'] = max(results['max_time'], elapsed)
            
            solution_moves = result['solution'].split()
            move_count = len(solution_moves)
            results['total_moves'] += move_count
            results['min_moves'] = min(results['min_moves'], move_count)
            results['max_moves'] = max(results['max_moves'], move_count)
            results['time_records'].append((elapsed, scramble, move_count))
            
            results['alll_total'] += 1
            alll_hit = bool(result.get('alll'))
            if alll_hit:
                results['alll_hit'] += 1
            else:
                # Record the scramble + cross+f2l as the case-generating formula
                case_formula = (result['cross'] + ' ' + result['f2l']).strip()
                results['alll_miss_cases'].append({
                    'scramble': scramble,
                    'cross': result['cross'],
                    'f2l': result['f2l'],
                    'oll': result['oll'],
                    'pll': result['pll'],
                    'case_formula': case_formula,
                })
            print(f"Status: SUCCESS")
            print(f"Time: {elapsed:.3f}s")
            print(f"Moves: {move_count}")
            print(f"ALLL  : {'HIT' if alll_hit else 'MISS'}")
            print(f"Cross ({len(result['cross'].split()) if result['cross'] else 0}): {result['cross'] or '-'}")
            print(f"F2L   ({len(result['f2l'].split()) if result['f2l'] else 0}): {result['f2l'] or '-'}")
            if alll_hit:
                print(f"ALLL  ({len(result['alll'].split()) if result['alll'] else 0}): {result['alll'] or '-'}")
            else:
                print(f"OLL   ({len(result['oll'].split()) if result['oll'] else 0}): {result['oll'] or '-'}")
                print(f"PLL   ({len(result['pll'].split()) if result['pll'] else 0}): {result['pll'] or '-'}")
            print(f"Full  ({move_count}): {result['solution']}")
    
    # Summary
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)
    print(f"Total tests: {num_tests}")
    print(f"Success: {results['success']} ({results['success']/num_tests*100:.1f}%)")
    print(f"Failed: {results['failed']} ({results['failed']/num_tests*100:.1f}%)")
    
    if results['success'] > 0:
        avg_time = results['total_time'] / results['success']
        avg_moves = results['total_moves'] / results['success']
        print(f"Average time: {avg_time:.3f}s (min={results['min_time']:.3f}s, max={results['max_time']:.3f}s)")
        print(f"Average moves: {avg_moves:.1f} (min={results['min_moves']}, max={results['max_moves']})")
        if results['alll_total'] > 0:
            alll_rate = results['alll_hit'] / results['alll_total'] * 100
            print(f"ALLL hit rate: {results['alll_hit']}/{results['alll_total']} ({alll_rate:.1f}%)")
        
        if results['alll_miss_cases']:
            print(f"\nALLL MISS cases ({len(results['alll_miss_cases'])}):")
            for idx, case in enumerate(results['alll_miss_cases'], 1):
                print(f"  #{idx}: scramble={case['scramble']}")
                # print(f"       cross+f2l = {case['case_formula']}")
                print(f"       generate = \n{case['scramble'] + case['case_formula']}\n")
                # print(f"       OLL={case['oll']}  PLL={case['pll']}")
        
        # Top 10 slowest
        top10 = sorted(results['time_records'], reverse=True)[:10]
        print(f"\nTop 10 slowest solves:")
        for rank, (t, scr, mc) in enumerate(top10, 1):
            print(f"  #{rank}: {t:.3f}s, {mc} moves, scramble: {scr}")
    
    if results['failed'] > 0:
        print(f"\nFailed scrambles:")
        for scramble in results['failed_scrambles']:
            print(f"  {scramble}")
    
    return results


if __name__ == '__main__':
    import sys
    
    # Parse command line arguments
    num_tests = 10
    scramble_length = 20
    enable_profile = False
    
    if len(sys.argv) > 1:
        num_tests = int(sys.argv[1])
    if len(sys.argv) > 2:
        scramble_length = int(sys.argv[2])
    if len(sys.argv) > 3 and sys.argv[3] == '--profile':
        enable_profile = True
    
    print(f"CFOP Solver - Random Scramble Test")
    print(f"Number of tests: {num_tests}")
    print(f"Scramble length: {scramble_length}")
    print(f"Profiling: {'Enabled' if enable_profile else 'Disabled'}")
    print()
    
    test_random_scrambles(num_tests, scramble_length, profile=enable_profile)
