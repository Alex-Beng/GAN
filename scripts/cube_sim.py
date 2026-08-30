"""Python mirror of src/protocol.rs cube state machine.

Replicates MOVE_TABLES / apply_move / to_facelet_string so tests can compute
expected /api/facelets and /api/state values offline.

NOTE: keep in sync with src/protocol.rs (MOVE_TABLES, CORNER_*, EDGE_* tables).
"""

FACE_NAMES = "URFDLB"
SOLVED_FACELETS = "U" * 9 + "R" * 9 + "F" * 9 + "D" * 9 + "L" * 9 + "B" * 9

# Per-face CW move tables: new[i] = old[src] (+ ori delta)
_CP = [
    [3, 0, 1, 2, 4, 5, 6, 7],    # U
    [4, 1, 2, 0, 7, 5, 6, 3],    # R
    [1, 5, 2, 3, 0, 4, 6, 7],    # F
    [0, 1, 2, 3, 5, 6, 7, 4],    # D
    [0, 2, 6, 3, 4, 1, 5, 7],    # L
    [0, 1, 3, 7, 4, 5, 2, 6],    # B
]
_CO = [
    [0, 0, 0, 0, 0, 0, 0, 0],    # U
    [2, 0, 0, 1, 1, 0, 0, 2],    # R
    [1, 2, 0, 0, 2, 1, 0, 0],    # F
    [0, 0, 0, 0, 0, 0, 0, 0],    # D
    [0, 1, 2, 0, 0, 2, 1, 0],    # L
    [0, 0, 1, 2, 0, 0, 2, 1],    # B
]
_EP = [
    [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],     # U
    [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],     # R
    [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],     # F
    [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],     # D
    [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],     # L
    [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],     # B
]
_EO = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],       # U
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],       # R
    [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],       # F
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],       # D
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],       # L
    [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],       # B
]

_CORNER_FACELET = [
    [8, 9, 20], [6, 18, 38], [0, 36, 47], [2, 45, 11],
    [29, 26, 15], [27, 44, 24], [33, 53, 42], [35, 17, 51],
]
_CORNER_COLOR = [
    [0, 1, 2], [0, 2, 4], [0, 4, 5], [0, 5, 1],
    [3, 2, 1], [3, 4, 2], [3, 5, 4], [3, 1, 5],
]
_EDGE_FACELET = [
    [5, 10], [7, 19], [3, 37], [1, 46], [32, 16], [28, 25],
    [30, 43], [34, 52], [23, 12], [21, 41], [50, 39], [48, 14],
]
_EDGE_COLOR = [
    [0, 1], [0, 2], [0, 4], [0, 5], [3, 1], [3, 2],
    [3, 4], [3, 5], [2, 1], [2, 4], [5, 4], [5, 1],
]
_CENTER_FACELET = [4, 13, 22, 31, 40, 49]


class CubeState:
    def __init__(self):
        self.cp = list(range(8))
        self.co = [0] * 8
        self.ep = list(range(12))
        self.eo = [0] * 12

    def clone(self):
        s = CubeState()
        s.cp = list(self.cp)
        s.co = list(self.co)
        s.ep = list(self.ep)
        s.eo = list(self.eo)
        return s


def apply_cw(st, face):
    ocp, oco, oep, oeo = list(st.cp), list(st.co), list(st.ep), list(st.eo)
    tcp, tco, tep, teo = _CP[face], _CO[face], _EP[face], _EO[face]
    for i in range(8):
        src = tcp[i]
        st.cp[i] = ocp[src]
        st.co[i] = (oco[src] + tco[i]) % 3
    for i in range(12):
        src = tep[i]
        st.ep[i] = oep[src]
        st.eo[i] = (oeo[src] + teo[i]) % 2


def apply_face_dir(st, face, d):
    """Apply (face, d) the same way the firmware queue does:
    d==0 -> one CW, d==1 -> three CW (CCW), d==2 -> two CW pushes."""
    turns = {0: 1, 1: 3, 2: 2}[d]
    for _ in range(turns):
        apply_cw(st, face)


def parse_notation(mv):
    """'R' -> (1,0), "R'" -> (1,1), 'R2' -> (1,2)"""
    if len(mv) == 1:
        return FACE_NAMES.index(mv), 0
    if mv[1] == "'":
        return FACE_NAMES.index(mv[0]), 1
    if mv[1] == "2":
        return FACE_NAMES.index(mv[0]), 2
    raise ValueError("bad notation: %r" % mv)


def notation(face, d):
    return FACE_NAMES[face] + ("", "'", "2")[d]


def apply_notation(st, mv):
    apply_face_dir(st, *parse_notation(mv))


def to_facelet_string(st):
    f = ["U"] * 54
    for i in range(6):
        f[_CENTER_FACELET[i]] = FACE_NAMES[i]
    for i in range(8):
        j, ori = st.cp[i], st.co[i]
        for k in range(3):
            f[_CORNER_FACELET[i][(k + ori) % 3]] = FACE_NAMES[_CORNER_COLOR[j][k]]
    for i in range(12):
        j, ori = st.ep[i], st.eo[i]
        for k in range(2):
            f[_EDGE_FACELET[i][(k + ori) % 2]] = FACE_NAMES[_EDGE_COLOR[j][k]]
    return "".join(f)


def state_dict(st):
    return {"cp": list(st.cp), "co": list(st.co),
            "ep": list(st.ep), "eo": list(st.eo)}


if __name__ == "__main__":
    st = CubeState()
    assert to_facelet_string(st) == SOLVED_FACELETS
    apply_notation(st, "U")
    assert to_facelet_string(st) != SOLVED_FACELETS
    for mv in ["U'", "R", "R'", "F2", "D", "D'", "L", "L'", "B2", "B2"]:
        apply_notation(st, mv)
    inv = ["B2", "B2", "L", "L'", "D", "D'", "F2", "R", "R'", "U'", "U'"]
    for mv in reversed(["U", "U'", "R", "R'", "F2", "D", "D'", "L", "L'", "B2", "B2"]):
        f, d = parse_notation(mv)
        d_inv = {0: 1, 1: 0, 2: 2}[d]
        apply_face_dir(st, f, d_inv)
    assert to_facelet_string(st) == SOLVED_FACELETS, "inverse check failed"
    print("cube_sim self-test OK")
