import os
import pickle
from collections import deque

SOLVED_FACELET = "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"

MOVE_CP = [
    [3, 0, 1, 2, 4, 5, 6, 7],
    [4, 1, 2, 0, 7, 5, 6, 3],
    [1, 5, 2, 3, 0, 4, 6, 7],
    [0, 1, 2, 3, 5, 6, 7, 4],
    [0, 2, 6, 3, 4, 1, 5, 7],
    [0, 1, 3, 7, 4, 5, 2, 6],
]

MOVE_CO = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [2, 0, 0, 1, 1, 0, 0, 2],
    [1, 2, 0, 0, 2, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 2, 0, 0, 2, 1, 0],
    [0, 0, 1, 2, 0, 0, 2, 1],
]

MOVE_EP = [
    [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
    [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
    [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
    [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
    [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
    [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
]

MOVE_EO = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
]

CORNER_FACELET = [
    [8, 9, 20],
    [6, 18, 38],
    [0, 36, 47],
    [2, 45, 11],
    [29, 26, 15],
    [27, 44, 24],
    [33, 53, 42],
    [35, 17, 51],
]

CORNER_COLOR = [
    [0, 1, 2],
    [0, 2, 4],
    [0, 4, 5],
    [0, 5, 1],
    [3, 2, 1],
    [3, 4, 2],
    [3, 5, 4],
    [3, 1, 5],
]

EDGE_FACELET = [
    [5, 10],
    [7, 19],
    [3, 37],
    [1, 46],
    [32, 16],
    [28, 25],
    [30, 43],
    [34, 52],
    [23, 12],
    [21, 41],
    [50, 39],
    [48, 14],
]

EDGE_COLOR = [
    [0, 1],
    [0, 2],
    [0, 4],
    [0, 5],
    [3, 1],
    [3, 2],
    [3, 4],
    [3, 5],
    [2, 1],
    [2, 4],
    [5, 4],
    [5, 1],
]

CENTER_FACELET = [4, 13, 22, 31, 40, 49]

COLOR_CHARS = ['U', 'R', 'F', 'D', 'L', 'B']

MOVE_NAMES = ['U', 'U2', "U'", 'R', 'R2', "R'", 'F', 'F2', "F'", 'D', 'D2', "D'", 'L', 'L2', "L'", 'B', 'B2', "B'"]

ROTATION_FACE_MAP = {
    'x': {'U': 'F', 'F': 'D', 'D': 'B', 'B': 'U', 'R': 'R', 'L': 'L'},
    'y': {'U': 'U', 'R': 'B', 'F': 'R', 'L': 'F', 'B': 'L', 'D': 'D'},
    'z': {'U': 'L', 'R': 'U', 'D': 'R', 'L': 'D', 'F': 'F', 'B': 'B'},
}

def apply_rotation(face, rot):
    return ROTATION_FACE_MAP[rot].get(face, face)

def convert_alg_to_basic(alg_str):
    tokens = alg_str.strip().split()
    result = []
    rotations = []

    for t in tokens:
        if not t:
            continue
        base = t[0]
        mod = t[1] if len(t) > 1 else ''
        count = {'': 1, "'": 3, '2': 2}.get(mod, 1)


        if base in 'URFDLB':
            face = base
            for r in rotations:
                face = apply_rotation(face, r)
            if count == 1:
                result.append(face)
            elif count == 2:
                result.append(face + '2')
            elif count == 3:
                result.append(face + "'")
        
        elif base in 'urfdlb':
            face = base.upper()
            lface = face
            for r in rotations:
                face = apply_rotation(face, r)
            base = face.lower()
            # print(f"base: {base}, olface: {lface}, new face: {face}, rotations: {rotations}, count {count}")
            if base == 'r':
                for _ in range(count % 4):
                    face = 'L'
                    result.append(face)
                for _ in range(count % 4):
                    # print(rotations)
                    rotations.append('x')

            elif base == 'l':
                for _ in range(count % 4):
                    face = 'R'
                    result.append(face)
                for _ in range(count % 4):
                    rotations.append('x')
                    rotations.append('x')
                    rotations.append('x')

            elif base == 'f':
                for _ in range(count % 4):
                    face = 'B'
                    result.append(face)
                for _ in range(count % 4):
                    rotations.append('z')

            elif base == 'b':
                for _ in range(count % 4):
                    face = 'F'
                    result.append(face)
                for _ in range(count % 4):
                    rotations.append('z')
                    rotations.append('z')
                    rotations.append('z')

            elif base == 'u':
                for _ in range(count % 4):
                    face = 'D'
                    result.append(face)
                for _ in range(count % 4):
                    rotations.append('y')

            elif base == 'd':
                for _ in range(count % 4):
                    face = 'U'
                    result.append(face)
                for _ in range(count % 4):
                    rotations.append('y')
                    rotations.append('y')
                    rotations.append('y')


        elif base == 'M':
            if count == 1:  # M
                face_l = 'L'
                face_r = 'R'
                for r in rotations:
                    face_l = apply_rotation(face_l, r)
                    face_r = apply_rotation(face_r, r)
                result.append(face_l + "'")
                result.append(face_r)
                rotations.append('x')
                rotations.append('x')
                rotations.append('x')
            elif count == 2:  # M2
                face_l = 'L'
                face_r = 'R'
                for r in rotations:
                    face_l = apply_rotation(face_l, r)
                    face_r = apply_rotation(face_r, r)
                result.append(face_l + "2")
                result.append(face_r + "2")
                rotations.append('x')
                rotations.append('x')
            elif count == 3:  # M'
                face_l = 'L'
                face_r = 'R'
                for r in rotations:
                    face_l = apply_rotation(face_l, r)
                    face_r = apply_rotation(face_r, r)
                result.append(face_l)
                result.append(face_r + "'")
                rotations.append('x')

        elif base == 'S':
            if count == 1:  # S
                face_f = 'F'
                face_b = 'B'
                for r in rotations:
                    face_f = apply_rotation(face_f, r)
                    face_b = apply_rotation(face_b, r)
                result.append(face_f + "'")
                result.append(face_b)
                rotations.append('z')
            elif count == 2:  # S2
                face_f = 'F'
                face_b = 'B'
                for r in rotations:
                    face_f = apply_rotation(face_f, r)
                    face_b = apply_rotation(face_b, r)
                result.append(face_f + "2")
                result.append(face_b + "2")
                rotations.append('z')
                rotations.append('z')
            elif count == 3:  # S'
                face_f = 'F'
                face_b = 'B'
                for r in rotations:
                    face_f = apply_rotation(face_f, r)
                    face_b = apply_rotation(face_b, r)
                result.append(face_f)
                result.append(face_b + "'")
                rotations.append('z')
                rotations.append('z')
                rotations.append('z')

        elif base == 'E':
            if count == 1:  # E
                face_d = 'D'
                face_u = 'U'
                for r in rotations:
                    face_d = apply_rotation(face_d, r)
                    face_u = apply_rotation(face_u, r)
                result.append(face_d + "'")
                result.append(face_u)
                rotations.append('y')
            elif count == 2:  # E2
                face_d = 'D'
                face_u = 'U'
                for r in rotations:
                    face_d = apply_rotation(face_d, r)
                    face_u = apply_rotation(face_u, r)
                result.append(face_d + "2")
                result.append(face_u + "2")
                rotations.append('y')
                rotations.append('y')
            elif count == 3:  # E'
                face_d = 'D'
                face_u = 'U'
                for r in rotations:
                    face_d = apply_rotation(face_d, r)
                    face_u = apply_rotation(face_u, r)
                result.append(face_d)
                result.append(face_u + "'")
                rotations.append('y')
                rotations.append('y')
                rotations.append('y')

        elif base in 'xyz':
            for _ in range(count % 4):
                rotations.append(base)

        else:
            face = base
            for r in rotations:
                face = apply_rotation(face, r)
            result.append(face + mod if mod else face)

    simplified = []
    i = 0
    while i < len(result):
        move = result[i]
        base = move[0]
        mod = move[1] if len(move) > 1 else ''
        total = {'': 1, "'": 3, '2': 2}.get(mod, 1)

        j = i + 1
        while j < len(result):
            next_move = result[j]
            next_base = next_move[0]
            if next_base != base:
                break
            next_mod = next_move[1] if len(next_move) > 1 else ''
            next_count = {'': 1, "'": 3, '2': 2}.get(next_mod, 1)
            total = (total + next_count) % 4
            j += 1

        if total == 1:
            simplified.append(base)
        elif total == 2:
            simplified.append(base + '2')
        elif total == 3:
            simplified.append(base + "'")
        i = j

    return ' '.join(simplified)

F2L_PAIRS = [
    (4, 8),
    (5, 9),
    (6, 10),
    (7, 11),
]

F2L_MOVE_INDICES = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 15, 16, 17]

OPPOSITE_FACE = {0: 3, 3: 0, 1: 4, 4: 1, 2: 5, 5: 2}


class CubieCube:
    __slots__ = ('cp', 'co', 'ep', 'eo')

    def __init__(self, cp=None, co=None, ep=None, eo=None):
        self.cp = list(cp) if cp else list(range(8))
        self.co = list(co) if co else [0] * 8
        self.ep = list(ep) if ep else list(range(12))
        self.eo = list(eo) if eo else [0] * 12

    @staticmethod
    def solved():
        return CubieCube()

    def copy(self):
        return CubieCube(self.cp, self.co, self.ep, self.eo)

    def apply_cw(self, face):
        t_cp = MOVE_CP[face]
        t_co = MOVE_CO[face]
        t_ep = MOVE_EP[face]
        t_eo = MOVE_EO[face]
        old_cp = list(self.cp)
        old_co = list(self.co)
        old_ep = list(self.ep)
        old_eo = list(self.eo)
        for i in range(8):
            src = t_cp[i]
            self.cp[i] = old_cp[src]
            self.co[i] = (old_co[src] + t_co[i]) % 3
        for i in range(12):
            src = t_ep[i]
            self.ep[i] = old_ep[src]
            self.eo[i] = (old_eo[src] + t_eo[i]) % 2

    def apply_move(self, face, dir):
        times = 1 if dir == 0 else 3  # dir=0: CW, dir=1: CCW (3 CW turns)
        for _ in range(times):
            self.apply_cw(face)

    def apply_move_idx(self, idx):
        face = idx // 3
        turn_type = idx % 3  # 0=CW, 1=half turn, 2=CCW
        if turn_type == 0:
            self.apply_cw(face)
        elif turn_type == 1:
            self.apply_cw(face)
            self.apply_cw(face)
        else:  # turn_type == 2: CCW
            self.apply_cw(face)
            self.apply_cw(face)
            self.apply_cw(face)

    def apply_move_idx_copy(self, idx):
        s = self.copy()
        s.apply_move_idx(idx)
        return s

    def _apply_rotation(self, axis):
        if axis == 'x':
            temp = self.cp[:]
            self.cp = [temp[i] for i in [4, 1, 2, 0, 7, 5, 6, 3]]
            temp = self.co[:]
            self.co = [(temp[i] + (2 if i in [0, 7] else 1 if i in [3, 4] else 0)) % 3 for i in [4, 1, 2, 0, 7, 5, 6, 3]]
            temp = self.ep[:]
            self.ep = [temp[i] for i in [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0]]
            temp = self.eo[:]
            self.eo = [temp[i] for i in [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0]]
        elif axis == 'y':
            temp = self.cp[:]
            self.cp = [temp[i] for i in [3, 0, 1, 2, 4, 5, 6, 7]]
            temp = self.co[:]
            self.co = [temp[i] for i in [3, 0, 1, 2, 4, 5, 6, 7]]
            temp = self.ep[:]
            self.ep = [temp[i] for i in [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11]]
            temp = self.eo[:]
            self.eo = [temp[i] for i in [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11]]
        elif axis == 'z':
            temp = self.cp[:]
            self.cp = [temp[i] for i in [1, 5, 2, 3, 0, 4, 6, 7]]
            temp = self.co[:]
            self.co = [(temp[i] + (1 if i in [0, 5] else 2 if i in [1, 4] else 0)) % 3 for i in [1, 5, 2, 3, 0, 4, 6, 7]]
            temp = self.ep[:]
            self.ep = [temp[i] for i in [0, 8, 2, 3, 4, 1, 6, 7, 5, 9, 10, 11]]
            temp = self.eo[:]
            new_eo = []
            for i in [0, 8, 2, 3, 4, 1, 6, 7, 5, 9, 10, 11]:
                idx_in_old = i
                if idx_in_old in [1, 5, 8]:
                    new_eo.append((temp[idx_in_old] + 1) % 2)
                else:
                    new_eo.append(temp[idx_in_old])
            self.eo = new_eo

    def apply_moves_str(self, moves_str):
        s = self.copy()
        for m in moves_str.strip().split():
            if not m:
                continue
            face = COLOR_CHARS.index(m[0])
            if len(m) == 1:
                s.apply_move(face, 0)
            elif m[1] == '2':
                s.apply_move(face, 0)
                s.apply_move(face, 0)
            elif m[1] == "'":
                s.apply_move(face, 1)
        return s

    @staticmethod
    def from_facelet(s):
        f = list(s)
        cc = [f[c] for c in CENTER_FACELET]
        cube = CubieCube()
        for i in range(8):
            for j in range(8):
                ori = 0
                for ori in range(3):
                    if all(f[CORNER_FACELET[i][(k + ori) % 3]] == COLOR_CHARS[CORNER_COLOR[j][k]] for k in range(3)):
                        cube.cp[i] = j
                        cube.co[i] = ori
                        break
        for i in range(12):
            for j in range(12):
                for ori in range(2):
                    if f[EDGE_FACELET[i][(0 + ori) % 2]] == COLOR_CHARS[EDGE_COLOR[j][0]] and \
                       f[EDGE_FACELET[i][(1 + ori) % 2]] == COLOR_CHARS[EDGE_COLOR[j][1]]:
                        cube.ep[i] = j
                        cube.eo[i] = ori
                        break
        return cube

    def to_facelet(self):
        f = list(SOLVED_FACELET)
        for i in range(6):
            f[CENTER_FACELET[i]] = COLOR_CHARS[i]
        for i in range(8):
            j = self.cp[i]
            ori = self.co[i]
            for k in range(3):
                f[CORNER_FACELET[i][(k + ori) % 3]] = COLOR_CHARS[CORNER_COLOR[j][k]]
        for i in range(12):
            j = self.ep[i]
            ori = self.eo[i]
            for k in range(2):
                f[EDGE_FACELET[i][(k + ori) % 2]] = COLOR_CHARS[EDGE_COLOR[j][k]]
        return ''.join(f)

    def is_solved(self):
        return self.cp == list(range(8)) and self.co == [0]*8 and self.ep == list(range(12)) and self.eo == [0]*12

    def apply_u_rot(self, count):
        s = self.copy()
        for _ in range(count):
            s.apply_cw(0)
        return s


def invert_move_str(m):
    if len(m) == 1:
        return m + "'"
    elif m[1] == "'":
        return m[0]
    elif m[1] == '2':
        return m
    return m


def invert_alg(alg):
    moves = alg.strip().split()
    return ' '.join(invert_move_str(m) for m in reversed(moves))


def comb_idx(chosen, n, k):
    cnk = _cnk_table()
    idx = 0
    r = k
    for i in range(n):
        if chosen[i]:
            idx += cnk[n - 1 - i][r]
            r -= 1
    return idx


def perm_idx(perm, n):
    idx = 0
    used = [False] * n
    for i in range(n):
        cnt = 0
        for j in range(perm[i]):
            if not used[j]:
                cnt += 1
        idx = idx * (n - i) + cnt
        used[perm[i]] = True
    return idx


def perm_from_idx(idx, n):
    perm = [0] * n
    used = [False] * n
    for i in range(n):
        fact_val = 1
        for k in range(1, n - i):
            fact_val *= k
        cnt = idx // fact_val
        idx = idx % fact_val
        j = 0
        while j < n:
            if not used[j]:
                if cnt == 0:
                    perm[i] = j
                    used[j] = True
                    break
                cnt -= 1
            j += 1
    return perm


_cnk_cache = None


def _cnk_table():
    global _cnk_cache
    if _cnk_cache is None:
        _cnk_cache = [[0] * 13 for _ in range(13)]
        for i in range(13):
            _cnk_cache[i][0] = 1
            for j in range(1, i + 1):
                _cnk_cache[i][j] = _cnk_cache[i-1][j-1] + _cnk_cache[i-1][j]
    return _cnk_cache


def get_cross_perm_coord(state):
    positions = [0] * 12
    pieces = [0] * 12
    r = 4
    comb_val = 0
    cnk = _cnk_table()
    perm_pieces = []
    for i in range(12):
        if state.ep[i] in (4, 5, 6, 7):
            comb_val += cnk[11 - i][r]
            r -= 1
            perm_pieces.append(state.ep[i] - 4)
    perm_order = perm_idx(perm_pieces, 4)
    return comb_val * 24 + perm_order


def get_cross_flip_coord(state):
    r = 4
    comb_val = 0
    cnk = _cnk_table()
    flip_bits = 0
    bit_pos = 0
    for i in range(12):
        if state.ep[i] in (4, 5, 6, 7):
            comb_val += cnk[11 - i][r]
            r -= 1
            if state.eo[i]:
                flip_bits |= (1 << bit_pos)
            bit_pos += 1
    return comb_val * 16 + flip_bits


N_CROSS_PERM = 495 * 24 # 12! / 4! * 4!
N_CROSS_FLIP = 495 * 16 # 12! / 2^4

PERM_MOVE = None
FLIP_MOVE = None
PERM_PRUN = None
FLIP_PRUN = None

PAIR_MOVE = None
PAIR_PRUN = None

OLL_ALGS = {
    1: "R U2 R2 F R F' U2 R' F R F'",
    2: "y' R U' R2 D' r U r' D R2 U R'",
    3: "r' R2 U R' U r U2 r' U M'",
    4: "y' R' F2 R2 U2 R' F' R U2 R2 F2 R",
    5: "r' U2 R U R' U r",
    6: "r U2 R' U' R U' r'",
    7: "r U R' U R U2 r'",
    8: "y2 r' U' R U' R' U2 r",
    9: "y R U R' U' R' F R2 U R' U' F'",
    10: "R U R' U R' F R F' R U2 R'",
    11: "r' R2 U R' U R U2 R' U M'",
    12: "y' M' R' U' R U' R' U2 R U' M",
    13: "F U R U2 R' U' R U R' F'",
    14: "R' F R U R' F' R F U' F'",
    15: "r' U' r R' U' R U r' U r",
    16: "r U r' R U R' U' r U' r'",
    17: "R U R' U R' F R F' U2 R' F R F'",
    18: "y R U2 R2 F R F' U2 M' U R U' r'",
    19: "y S' R U R' S U' R' F R F'",
    20: "r U R' U' M2 U R U' R' U' M'",
    21: "R U R' U R U' R' U R U2 R'",
    22: "R U2 R2 U' R2 U' R2 U2 R",
    23: "R2 D R' U2 R D' R' U2 R'",
    24: "r U R' U' r' F R F'",
    25: "R U2 R D R' U2 R D' R2",
    26: "y R U2 R' U' R U' R'",
    27: "R U R' U R U2 R'",
    28: "r U R' U' M U R U' R'",
    29: "r2 D' r U r' D r2 U' r' U' r",
    30: "y' r' D' r U' r' D r2 U' r' U r U r'",
    31: "R' U' F U R U' R' F' R",
    32: "S R U R' U' R' F R f'",
    33: "R U R' U' R' F R F'",
    34: "y f R f' U' r' U' R U M'",
    35: "R U2 R2 F R F' R U2 R'",
    36: "y R U R2 F' U' F U R2 U2 R'",
    37: "F R' F' R U R U' R'",
    38: "R U R' U R U' R' U' R' F R F'",
    39: "y' f' r U r' U' r' F r S",     
    40: "y R' F R U R' U' F' U R",      
    41: "y2 R U R' U R U2 R' F R U R' U' F'",
    42: "R' U' R U' R' U2 R F R U R' U' F'", 
    43: "y R' U' F' U F R",   
    44: "f R U R' U' f'",     
    45: "F R U R' U' F'",     
    46: "R' U' R' F R F' U R",
    47: "y' F R' F' R U2 R U' R' U R U2 R'", 
    48: "F R U R' U' R U R' U' F'",     
    49: "y2 r U' r2 U r2 U r2 U' r",    
    50: "r' U r2 U' r2 U' r2 U r'",     
    51: "y2 F U R U' R' U R U' R' F'",  
    52: "y2 R' F' U' F U' R U R' U R",  
    53: "r' U' R U' R' U R U' R' U2 r", 
    54: "r U R' U R U' R' U R U2 r'",   
    55: "y R' F U R U' R2 F' R2 U R' U' R",  
    56: "r U r' U R U' R' U R U' R' r U' r'",
    57: "R U R' U' M' U R U' r'",       
}

PLL_ALGS = {
    'Aa': "x R' U R' D2 R U' R' D2 R2 x'",
    'Ab': "x R2 D2 R U R' D2 R U' R x'",
    'E': "x' R U' R' D R U R' D' R U R' D R U' R' D' x",
    'F': "R' U' F' R U R' U' R' F R2 U' R' U' R U R' U R",
    'Ga': "R2 U R' U R' U' R U' R2 D U' R' U R D'",
    'Gb': "R' U' R U D' R2 U R' U R U' R U' R2 D",
    'Gc': "R2 U' R U' R U R' U R2 D' U R U' R' D",
    'Gd': "R U R' U' D R2 U' R U' R' U R' U R2 D'",
    'H': "M2 U M2 U2 M2 U M2",
    'Ja': "R' U L' U2 R U' R' U2 R L x'",
    'Jb': "R U R' F' R U R' U' R' F R2 U' R'",
    'Na': "R U R' U R U R' F' R U R' U' R' F R2 U' R' U2 R U' R'",
    'Nb': "R' U R U' R' F' U' F R U R' F R' F' R U' R",
    'Ra': "R U R' F' R U2 R' U2 R' F R U R U2 R'",
    'Rb': "R' U2 R U2 R' F R U R' U' R' F' R2",
    'T': "R U R' U' R' F R2 U' R' U' R U R' F'",
    'Ua': "R U' R U R U R U' R' U' R2",
    'Ub': "R2 U R U R' U' R' U' R' U R'",
    'V': "R' U R' U' R D' R' D R' U D' R2 U' R2 D R2",
    'Y': "F R U' R' U' R U R' F' R U R' U' R' F R F'",
    'Z': "M' U M2 U M2 U M' U2 M2",
}

def expand_rotations(tokens):
    return tokens

def apply_alg_str(state, alg_str):
    basic_alg = convert_alg_to_basic(alg_str)
    tokens = basic_alg.strip().split()
    s = state.copy()
    for t in tokens:
        if not t:
            continue
        base = t[0]
        mod = t[1] if len(t) > 1 else ''
        face = COLOR_CHARS.index(base)
        count = {'': 1, '2': 2, "'": 3}.get(mod, 1)
        for _ in range(count):
            s.apply_cw(face)
    return s


EDGE_MOVE_PERM = [
    [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
    [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
    [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
    [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
    [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
    [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
]

EDGE_MOVE_EO = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
]


def _apply_edge_move_full(ep, eo, face):
    new_ep = [ep[EDGE_MOVE_PERM[face][i]] for i in range(12)]
    new_eo = [(eo[EDGE_MOVE_PERM[face][i]] + EDGE_MOVE_EO[face][i]) % 2 for i in range(12)]
    return new_ep, new_eo


def build_cross_move_tables():
    perm_move = [[0] * N_CROSS_PERM for _ in range(18)]
    flip_move = [[0] * N_CROSS_FLIP for _ in range(18)]

    solved_ep = list(range(12))
    solved_eo = [0] * 12

    all_states = {}
    for face in range(6):
        for turn in range(1, 4):
            ep, eo = list(solved_ep), list(solved_eo)
            for _ in range(turn):
                ep, eo = _apply_edge_move_full(ep, eo, face)

    for move_idx in range(18):
        face = move_idx // 3
        turns = (move_idx % 3) + 1

        for ps in range(N_CROSS_PERM):
            ep, eo = _decode_perm_to_edge(ps)
            for _ in range(turns):
                ep, eo = _apply_edge_move_full(ep, eo, face)
            perm_move[move_idx][ps] = _encode_perm_from_edge(ep, eo)

        for fs in range(N_CROSS_FLIP):
            ep, eo = _decode_flip_to_edge(fs)
            for _ in range(turns):
                ep, eo = _apply_edge_move_full(ep, eo, face)
            flip_move[move_idx][fs] = _encode_flip_from_edge(ep, eo)

    return perm_move, flip_move


def _decode_perm_to_edge(ps):
    comb = ps // 24
    perm_order = perm_from_idx(ps % 24, 4)
    cnk = _cnk_table()
    # Use non-cross values for initial positions to avoid duplicates with cross pieces (4-7)
    non_cross = [0, 1, 2, 3, 8, 9, 10, 11]
    ep = list(range(12))
    eo = [0] * 12
    # First, place non-cross pieces at positions that won't be cross positions
    cross_pos = []
    comb_copy = comb
    r_copy = 4
    for i in range(12):
        if comb_copy >= cnk[11 - i][r_copy]:
            comb_copy -= cnk[11 - i][r_copy]
            cross_pos.append(i)
            r_copy -= 1
    non_cross_idx = 0
    for i in range(12):
        if i not in cross_pos:
            ep[i] = non_cross[non_cross_idx]
            non_cross_idx += 1
    # Now place cross pieces at their positions
    for idx, pos in enumerate(cross_pos):
        ep[pos] = perm_order[idx] + 4
        eo[pos] = 0
    return ep, eo


def _decode_flip_to_edge(fs):
    comb = fs // 16
    flip_bits = fs % 16
    cnk = _cnk_table()
    # Use non-cross values for initial positions to avoid duplicates with cross pieces (4-7)
    non_cross = [0, 1, 2, 3, 8, 9, 10, 11]
    ep = list(range(12))
    eo = [0] * 12
    # Determine which positions hold cross pieces
    cross_pos = []
    comb_copy = comb
    r_copy = 4
    for i in range(12):
        if comb_copy >= cnk[11 - i][r_copy]:
            comb_copy -= cnk[11 - i][r_copy]
            cross_pos.append(i)
            r_copy -= 1
    non_cross_idx = 0
    for i in range(12):
        if i not in cross_pos:
            ep[i] = non_cross[non_cross_idx]
            eo[i] = 0
            non_cross_idx += 1
    # Now place cross pieces with flip info
    piece = 4
    bit_pos = 0
    for pos in cross_pos:
        ep[pos] = piece
        eo[pos] = (flip_bits >> bit_pos) & 1
        piece += 1
        bit_pos += 1
    return ep, eo


def _encode_perm_from_edge(ep, eo):
    cnk = _cnk_table()
    r = 4
    comb_val = 0
    perm_pieces = []
    for i in range(12):
        if ep[i] >= 4 and ep[i] <= 7:
            comb_val += cnk[11 - i][r]
            perm_pieces.append(ep[i] - 4)
            r -= 1
    perm_order = perm_idx(perm_pieces, 4)
    return comb_val * 24 + perm_order


def _encode_flip_from_edge(ep, eo):
    cnk = _cnk_table()
    r = 4
    comb_val = 0
    flip_bits = 0
    bit_pos = 0
    for i in range(12):
        if ep[i] >= 4 and ep[i] <= 7:
            comb_val += cnk[11 - i][r]
            if eo[i]:
                flip_bits |= (1 << bit_pos)
            bit_pos += 1
            r -= 1
    return comb_val * 16 + flip_bits


def build_cross_prun(perm_move, flip_move):
    # The solved cross state has edges 4,5,6,7 at D-layer positions 4,5,6,7
    # We need to find the correct coords for this state
    solved = CubieCube.solved()
    solved_perm = get_cross_perm_coord(solved)
    solved_flip = get_cross_flip_coord(solved)

    perm_prun = bytearray([0xff] * N_CROSS_PERM)
    perm_prun[solved_perm] = 0
    queue = deque([solved_perm])
    while queue:
        idx = queue.popleft()
        d = perm_prun[idx]
        if d >= 5:
            continue
        for m in range(18):
            nxt = perm_move[m][idx]
            if perm_prun[nxt] == 0xff:
                perm_prun[nxt] = d + 1
                queue.append(nxt)

    flip_prun = bytearray([0xff] * N_CROSS_FLIP)
    flip_prun[solved_flip] = 0
    queue = deque([solved_flip])
    while queue:
        idx = queue.popleft()
        d = flip_prun[idx]
        if d >= 6:
            continue
        for m in range(18):
            nxt = flip_move[m][idx]
            if flip_prun[nxt] == 0xff:
                flip_prun[nxt] = d + 1
                queue.append(nxt)

    return perm_prun, flip_prun


def get_pair_coord(state, pair_idx):
    corner_piece, edge_piece = F2L_PAIRS[pair_idx]
    cp_slot = state.cp.index(corner_piece)
    co_val = state.co[cp_slot]
    ep_slot = state.ep.index(edge_piece)
    eo_val = state.eo[ep_slot]
    return (cp_slot * 3 + co_val) * 24 + (ep_slot * 2 + eo_val)


def pair_is_solved(state, pair_idx):
    corner_piece, edge_piece = F2L_PAIRS[pair_idx]
    cp_slot = state.cp.index(corner_piece)
    ep_slot = state.ep.index(edge_piece)
    return cp_slot == corner_piece and state.co[cp_slot] == 0 and \
           ep_slot == edge_piece and state.eo[ep_slot] == 0


def pair_solved_coord(pair_idx):
    corner_piece, edge_piece = F2L_PAIRS[pair_idx]
    return (corner_piece * 3 + 0) * 24 + (edge_piece * 2 + 0)


def build_pair_move_tables():
    pair_move = [[[0] * 576 for _ in range(15)] for _ in range(4)]
    for pair_idx in range(4):
        for mi, move_idx in enumerate(F2L_MOVE_INDICES):
            face = move_idx // 3
            turns = (move_idx % 3) + 1
            for pc in range(576):
                s = _pair_to_cube(pc, pair_idx)
                for _ in range(turns):
                    s.apply_cw(face)
                pair_move[pair_idx][mi][pc] = get_pair_coord(s, pair_idx)
    return pair_move


def _pair_to_cube(pc, pair_idx):
    corner_piece, edge_piece = F2L_PAIRS[pair_idx]
    cp_slot = pc // (24 * 3)
    co_val = (pc // 24) % 3
    ep_slot = (pc % 24) // 2
    eo_val = pc % 2
    cube = CubieCube()
    # Clear the slot that will hold the pair piece, and place pair piece at its slot
    # We need to ensure corner_piece only appears once in cp, edge_piece only once in ep
    # Remove the pair piece from wherever it was by default
    for i in range(8):
        if cube.cp[i] == corner_piece and i != cp_slot:
            # Swap: put whatever was at cp_slot into this position
            cube.cp[i] = cube.cp[cp_slot]
            break
    cube.cp[cp_slot] = corner_piece
    cube.co[cp_slot] = co_val

    for i in range(12):
        if cube.ep[i] == edge_piece and i != ep_slot:
            cube.ep[i] = cube.ep[ep_slot]
            break
    cube.ep[ep_slot] = edge_piece
    cube.eo[ep_slot] = eo_val
    return cube


def build_pair_prun(pair_move):
    pair_prun = [bytearray([0xff] * 576) for _ in range(4)]
    for pair_idx in range(4):
        solved = pair_solved_coord(pair_idx)
        pair_prun[pair_idx][solved] = 0
        queue = deque([solved])
        while queue:
            idx = queue.popleft()
            d = pair_prun[pair_idx][idx]
            if d >= 8:
                continue
            for mi in range(15):
                nxt = pair_move[pair_idx][mi][idx]
                if pair_prun[pair_idx][nxt] == 0xff:
                    pair_prun[pair_idx][nxt] = d + 1
                    queue.append(nxt)
    return pair_prun


def get_next_moves(all_moves, last_face):
    allowed = []
    for m in all_moves:
        face = m // 3
        if face == last_face:
            continue
        opp = OPPOSITE_FACE.get(last_face, -1)
        if face == opp and last_face > opp:
            continue
        allowed.append(m)
    return allowed


def solve_cross(state, perm_prun, flip_prun, perm_move, flip_move, max_depth=8):
    perm0 = get_cross_perm_coord(state)
    flip0 = get_cross_flip_coord(state)
    if perm_prun[perm0] == 0 and flip_prun[flip0] == 0:
        return []
    for depth in range(1, max_depth + 1):
        path = []
        if _cross_ida(perm0, flip0, depth, -1, path, perm_prun, flip_prun, perm_move, flip_move):
            return path
    return None


def _cross_ida(perm, flip, remaining, last_face, path, perm_prun, flip_prun, perm_move, flip_move):
    h = max(perm_prun[perm], flip_prun[flip])
    if h > remaining:
        return False
    if remaining == 0:
        return perm_prun[perm] == 0 and flip_prun[flip] == 0
    next_moves = get_next_moves(range(18), last_face)
    for m in next_moves:
        np = perm_move[m][perm]
        nf = flip_move[m][flip]
        path.append(m)
        if _cross_ida(np, nf, remaining - 1, m // 3, path, perm_prun, flip_prun, perm_move, flip_move):
            return True
        path.pop()
    return False


def _check_cross_intact(state):
    for e in [4, 5, 6, 7]:
        if state.ep.index(e) != e or state.eo[state.ep.index(e)] != 0:
            return False
    return True


def solve_f2l_pair(state, pair_idx, pair_prun, pair_move, max_depth=10, protected_pairs=None):
    if protected_pairs is None:
        protected_pairs = set()
    if pair_is_solved(state, pair_idx):
        if _check_cross_intact(state) and all(pair_is_solved(state, pp) for pp in protected_pairs):
            return []
    for depth in range(1, max_depth + 1):
        path = []
        if _f2l_ida(state, pair_idx, depth, -1, path, pair_prun, pair_move, protected_pairs):
            return path
    return None


def _f2l_ida(state, pair_idx, remaining, last_face, path, pair_prun, pair_move, protected_pairs):
    h = pair_prun[pair_idx][get_pair_coord(state, pair_idx)]
    if h > remaining:
        return False
    if remaining == 0:
        if not pair_is_solved(state, pair_idx):
            return False
        for pp in protected_pairs:
            if not pair_is_solved(state, pp):
                return False
        if not _check_cross_intact(state):
            return False
        return True
    next_moves = get_next_moves(F2L_MOVE_INDICES, last_face)
    for mi, m in enumerate(next_moves):
        ns = state.apply_move_idx_copy(m)
        path.append(m)
        if _f2l_ida(ns, pair_idx, remaining - 1, m // 3, path, pair_prun, pair_move, protected_pairs):
            return True
        path.pop()
    return False


def solve_f2l(state, pair_prun, pair_move):
    full_moves = []
    solved_pairs = set()
    for i in range(4):
        if pair_is_solved(state, i):
            solved_pairs.add(i)
    for step in range(4):
        remaining = [i for i in range(4) if i not in solved_pairs]
        if not remaining:
            break
        # Try each remaining pair with U adjustments
        best_path = None
        best_pair = None
        best_prefix = []
        found = False
        for pair_idx in remaining:
            for auf in range(4):
                test_state = state.apply_u_rot(auf)
                # Check if this pair is already solved after U rotation
                if pair_is_solved(test_state, pair_idx):
                    best_path = []
                    best_pair = pair_idx
                    best_prefix = [0] * auf if auf > 0 else []
                    found = True
                    break
                # Try to solve with increasing depth, but cap at 8 for speed
                path = solve_f2l_pair(test_state, pair_idx, pair_prun, pair_move, max_depth=8, protected_pairs=solved_pairs)
                if path is not None:
                    best_path = path
                    best_pair = pair_idx
                    best_prefix = [0] * auf if auf > 0 else []
                    found = True
                    break
            if found:
                break
        
        if not found:
            # Fallback: try deeper search for first remaining pair
            pair_idx = remaining[0]
            for auf in range(4):
                test_state = state.apply_u_rot(auf)
                path = solve_f2l_pair(test_state, pair_idx, pair_prun, pair_move, max_depth=10, protected_pairs=solved_pairs)
                if path is not None:
                    best_path = path
                    best_pair = pair_idx
                    best_prefix = [0] * auf if auf > 0 else []
                    found = True
                    break
        
        if not found:
            return None
        
        full_moves.extend(best_prefix)
        full_moves.extend(best_path)
        for m in best_prefix + best_path:
            state.apply_move_idx(m)
        
        if not pair_is_solved(state, best_pair):
            return None
        for pp in solved_pairs:
            if not pair_is_solved(state, pp):
                return None
        solved_pairs.add(best_pair)

    # Verify F2L is complete: top 4 corners (0,1,2,3) should be in top positions with any orientation
    # and top 4 edges (0,1,2,3) should be in top positions with any orientation
    top_corners = set(state.cp[i] for i in range(4))
    top_edges = set(state.ep[i] for i in range(4))
    if top_corners != {0, 1, 2, 3}:
        print(f"DEBUG: F2L failed - top corners={list(state.cp[:4])}, expected {{0,1,2,3}}")
        return None
    if top_edges != {0, 1, 2, 3}:
        print(f"DEBUG: F2L failed - top edges={list(state.ep[:4])}, expected {{0,1,2,3}}")
        return None

    return full_moves, state


OLL_RECOGNITION = None
PLL_RECOGNITION = None


def build_oll_recognition():
    table = {}
    solved = CubieCube.solved()

    # Build table by applying each OLL algorithm's inverse (without AUF variations)
    for name, alg in OLL_ALGS.items():
        basic_alg = convert_alg_to_basic(alg)
        inv = invert_alg(basic_alg)
        s = apply_alg_str(solved.copy(), inv)
        co_tuple = tuple(s.co[i] for i in range(4))
        eo_tuple = tuple(s.eo[i] for i in range(4))
        key = (co_tuple, eo_tuple)

        # Don't overwrite - first one wins
        if key not in table:
            table[key] = (name, basic_alg)

    # Add solved state
    solved_key = ((0,0,0,0),(0,0,0,0))
    table[solved_key] = 'skip'

    return table


def build_pll_recognition():
    table = {}
    solved = CubieCube.solved()
    key_skip = (tuple(range(4)), tuple(range(4)))
    for name, alg in PLL_ALGS.items():
        basic_alg = convert_alg_to_basic(alg)
        for auf in range(4):
            auf_str = {0: '', 1: 'U', 2: 'U2', 3: "U'"}[auf]
            full_alg = (auf_str + ' ' + basic_alg).strip()
            s = apply_alg_str(solved.copy(), full_alg)
            cp_key = tuple(s.cp[i] for i in range(4))
            ep_key = tuple(s.ep[i] for i in range(4))
            table[(cp_key, ep_key)] = (name, basic_alg, auf)
    table[key_skip] = ('skip', '', 0)
    return table


def recognize_oll(state, oll_rec):
    # Try all 4 AUF rotations
    for auf in range(4):
        test = state.apply_u_rot(auf)
        co_tuple = tuple(test.co[i] for i in range(4))
        eo_tuple = tuple(test.eo[i] for i in range(4))
        key = (co_tuple, eo_tuple)

        if key in oll_rec:
            result = oll_rec[key]
            if result == 'skip':
                return 'skip', '', auf, None
            return result[0], result[1], auf, key

    return None, None, 0, None


def recognize_pll(state, pll_rec):
    for auf in range(4):
        test = state.apply_u_rot(auf)
        cp_key = tuple(test.cp[i] for i in range(4))
        ep_key = tuple(test.ep[i] for i in range(4))
        key = (cp_key, ep_key)
        if key in pll_rec:
            result = pll_rec[key]
            if result[0] == 'skip':
                return 'skip', '', auf
            return result[0], result[1], auf
    return None, None, 0


def solve_oll(state, oll_rec):
    name, alg, auf, key = recognize_oll(state, oll_rec)
    if name is not None:
        print(f"DEBUG: recognized OLL case={name} auf={auf} key={key}")
        auf_str = {0: '', 1: 'U', 2: 'U2', 3: "U'"}.get(auf, '')
        full = (auf_str + ' ' + alg).strip() if name != 'skip' else ''
        if full:
            state = apply_alg_str(state, full)
        return full, state

    print(f"DEBUG: OLL not recognized, state co={tuple(state.co[:4])} eo={tuple(state.eo[:4])} cp={tuple(state.cp[:4])} ep={tuple(state.ep[:4])}")
    return None, state


def solve_pll(state, pll_rec):
    name, alg, auf = recognize_pll(state, pll_rec)
    if name is None:
        return None, state
    auf_str = {0: '', 1: 'U', 2: 'U2', 3: "U'"}[auf]
    full = (auf_str + ' ' + alg).strip() if name != 'skip' else ''
    if full:
        state = apply_alg_str(state, full)
    if not state.is_solved():
        for post_auf in range(1, 4):
            test = state.apply_u_rot(post_auf)
            if test.is_solved():
                full = (full + ' ' + {1: 'U', 2: 'U2', 3: "U'"}[post_auf]).strip()
                state = test
                break
    return full, state


CACHE_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), '.cfop_cache.pkl')

_tables_initialized = False


def init_tables():
    global PERM_MOVE, FLIP_MOVE, PERM_PRUN, FLIP_PRUN, PAIR_MOVE, PAIR_PRUN, OLL_RECOGNITION, PLL_RECOGNITION, _tables_initialized
    if _tables_initialized:
        return
    if os.path.exists(CACHE_FILE): # debug先禁用掉
        with open(CACHE_FILE, 'rb') as f:
            tables = pickle.load(f)
        PERM_MOVE = tables['perm_move']
        FLIP_MOVE = tables['flip_move']
        PERM_PRUN = tables['perm_prun']
        FLIP_PRUN = tables['flip_prun']
        PAIR_MOVE = tables['pair_move']
        PAIR_PRUN = tables['pair_prun']
        OLL_RECOGNITION = tables['oll_rec']
        PLL_RECOGNITION = tables['pll_rec']
    else:
        print("Building CFOP tables (first run)...", flush=True)
        perm_move, flip_move = build_cross_move_tables()
        PERM_MOVE = perm_move
        FLIP_MOVE = flip_move
        perm_prun, flip_prun = build_cross_prun(perm_move, flip_move)
        PERM_PRUN = perm_prun
        FLIP_PRUN = flip_prun
        pair_move = build_pair_move_tables()
        PAIR_MOVE = pair_move
        pair_prun = build_pair_prun(pair_move)
        PAIR_PRUN = pair_prun
        oll_rec = build_oll_recognition()
        pll_rec = build_pll_recognition()
        OLL_RECOGNITION = oll_rec
        PLL_RECOGNITION = pll_rec
        tables = {
            'perm_move': perm_move,
            'flip_move': flip_move,
            'perm_prun': perm_prun,
            'flip_prun': flip_prun,
            'pair_move': pair_move,
            'pair_prun': pair_prun,
            'oll_rec': oll_rec,
            'pll_rec': pll_rec,
        }
        with open(CACHE_FILE, 'wb') as f:
            pickle.dump(tables, f)
    _tables_initialized = True


def moves_to_str(move_indices):
    return ' '.join(MOVE_NAMES[m] for m in move_indices) if move_indices else ''


class CFOPSolver:
    def __init__(self):
        init_tables()

    def solve(self, facelet_str):
        state = CubieCube.from_facelet(facelet_str)
        if state.is_solved():
            return {
                'cross': '',
                'f2l': '',
                'oll': '',
                'pll': '',
                'solution': '',
            }

        cross_moves = solve_cross(state, PERM_PRUN, FLIP_PRUN, PERM_MOVE, FLIP_MOVE)
        if cross_moves is None:
            print("DEBUG: cross failed")
            return None
        cross_str = moves_to_str(cross_moves)
        print(f"DEBUG: cross = {cross_str}")
        for m in cross_moves:
            state.apply_move_idx(m)

        f2l_result = solve_f2l(state, PAIR_PRUN, PAIR_MOVE)
        if f2l_result is None:
            print("DEBUG: f2l failed")
            return None
        f2l_moves, state = f2l_result
        f2l_str = moves_to_str(f2l_moves)
        print(f"DEBUG: f2l = {f2l_str}")

        oll_str, state = solve_oll(state, OLL_RECOGNITION)
        if oll_str is None:
            print("DEBUG: oll failed")
            f = state.to_facelet()
            u_pattern = ''.join('1' if f[i] == 'U' else '0' for i in range(9))
            print(f"DEBUG: U-face pattern: {u_pattern}")
            print(f"DEBUG: U-face colors: {f[:9]}")
            for auf in range(4):
                test = state.apply_u_rot(auf)
                f2 = test.to_facelet()
                p2 = ''.join('1' if f2[i] == 'U' else '0' for i in range(9))
                print(f"DEBUG: auf={auf} pattern={p2} in_table={p2 in OLL_RECOGNITION}")
            return None
        print(f"DEBUG: oll = {oll_str}")

        pll_str, state = solve_pll(state, PLL_RECOGNITION)
        if pll_str is None:
            print("DEBUG: pll failed")
            print(f"DEBUG: after oll cp={state.cp} co={state.co} ep={state.ep} eo={state.eo}")
            for auf in range(4):
                test = state.apply_u_rot(auf)
                cp_key = tuple(test.cp[i] for i in range(4))
                ep_key = tuple(test.ep[i] for i in range(4))
                key = (cp_key, ep_key)
                print(f"DEBUG: auf={auf} key={key} in_table={key in PLL_RECOGNITION}")
            return None
        print(f"DEBUG: pll = {pll_str}")

        solution_parts = [cross_str, f2l_str, oll_str, pll_str]
        solution = ' '.join(p for p in solution_parts if p)

        return {
            'cross': cross_str,
            'f2l': f2l_str,
            'oll': oll_str,
            'pll': pll_str,
            'solution': solution,
        }


def main():
    import sys
    import time
    if len(sys.argv) < 2:
        print("Usage: python cfop_solver.py [--scramble] <facelet_or_scramble>")
        print("  Default: treat input as facelet string")
        print("  --scramble: treat input as scramble moves")
        sys.exit(1)

    use_scramble = False
    if sys.argv[1] == '--scramble':
        use_scramble = True
        if len(sys.argv) < 3:
            print("Error: scramble input required")
            sys.exit(1)
        scramble_input = sys.argv[2]
    else:
        scramble_input = None

    if use_scramble:
        c1 = CubieCube.solved()
        c2 = apply_alg_str(c1, scramble_input)
        facelet = c2.to_facelet()
        print(f"Scramble: {scramble_input}")
        print(f"Facelet: {facelet}")
    else:
        facelet = sys.argv[1]
        if len(facelet) != 54:
            print("Invalid facelet string length")
            sys.exit(1)

    solver = CFOPSolver()
    start_time = time.time()
    result = solver.solve(facelet)
    elapsed_time = time.time() - start_time

    if result is None:
        print("No solution found")
        sys.exit(1)
    print(f"Cross: {result['cross']}")
    print(f"F2L: {result['f2l']}")
    print(f"OLL: {result['oll']}")
    print(f"PLL: {result['pll']}")
    print(f"Solution: {result['solution']}")
    print(f"Time: {elapsed_time:.3f}s")


if __name__ == '__main__':
    # FBLFURFBBRDUDRBRFBLLUUFFUBFRDDLDRLLRDUULLRBDFBULRBUDFD
    main()
