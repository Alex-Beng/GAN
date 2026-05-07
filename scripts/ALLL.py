# ALL 1LLL algs to enhance perf
# FYI: https://drive.google.com/drive/folders/1HcO-MxEb0U6nVGBpqWhsBoRO6eZdw6bo

EX_ALLLS = {}

def update_all(perfix, algs_str):
    global EX_ALLLS
    algs = algs_str.splitlines()[1:]
    EX_ALLLS.update({f"{perfix}_{i+1}": algs[i] for i in range(len(algs)) })

AU = """
"U2 R' U2' (R2 U R' U') R' U2' (r U R U' r')
U2 (R' F R U' M' U2 M) (U' S R' F' R S')"
"U' (L F' L' S') (L F L' S)
U (S' R U' R') (S R U R')
U2 (r U R' U' M) (U R U' R')"
"U2 (R' F R S) (R' F' R S')
U' (l' U' L U M) (U' L' U L)"
"U2 M U M' U2 M U M'
M' U M U2 M' U M"
U' M U' M' U2 M U' M'
U' (M' U M U)2 M' U M
"(L' U2 L2 U F' L') (D' L2' D F L U' L')
U2 (L' U' L U' L' U2 L) U' (r U r' U R U' R' r U' r')"
"U (R U2' R2' U' F R) (D R2 D' F' R' U R)
U' (R U R' U R U2' R') U (l' U' l U' L' U L l' U l)"
U (R' U' F' U F R) U' (f R U R' U' f')
(F R U R' U' F') (R' U' R' F R F' U R)
U2 M' U' M2' U' M U2 M' U' M'
U' M' U M2' U M U2 M' U M'
"""

AD = """
"(R' U' R U' R' U2' R) U (R' U2' R U R' U F' U' F U R)
U' F (R U2' R' U) F' R U' r' F2 U F' U' M'
U L2' D' (l U' l' D) L2 U F U F'
R2 D (r' U r D') R2' U' F' U' F
U' (F U' F' U) (L' U L U) (L' U' L U2') F U2' F'
U' r2 D (r' U r D') r2' B (r U' r' B')
↑r2' D' (r U' r' D) r2 F' (r' U r F)
U2 (F' U F U') (R U' R' U') (R U R' U2) F' U2 F
U' (R U2' R' U2 R' F R F') (R U' L' U R' U' L)
U (R U R' U) (R' F R F') (R' U2' R2 U R2' U R2 U' R')
U F (R U' R' F) (R' F' R U2) (R' U' F' U F R) F'
(R' F R U R' U' F' U R2) (U R' U' R' F R F')
"""

AB = """
U M(R U R' F' R U R' U' R' F R2 U' R' U') M'
U' (L U F U' F'·L) (D L' U2' L D' L' U2' L')
"U' (l' U' L U M) (U' F L' U' L U L F' L2' U L)
[oh] (F R U R' U') (R' F' U' F R F' U' R' U2' R)"
U' (F' L F U' L') U2' (L' U2 L U') L U2 L'
"U' (F' L F L' U2' L' U2 L) U (L' U2 L U L' U L)
U2 R' F U F (R U2 R' U') F' U'↓R' F2 R2"
"U2 (R u R' U R U' R u' R2') U' (R f R' f')
U' F' (L' U' L2 U L') (U' L' U L F) (U L U' L')
U r' U F2 U F (R' F R U) F' U2 F r"
U2 (r U R2' F' R) (U R U' R') (F R U' r')
U' (L U L' U') (L U L' U2') (L F' L F L2') (U' L U' L')
U' (l' U' L U) (l F' L2' U L U) (L' U' L F)
U' (L' U L U2') (L U F' L' F) (L U' L2' U L)
U' (F' L F L') U2' (L' U L U') (L U L2' U L2 U2 L')
U' (L F' L' F) (L F' L2' U' L U) (L F L2' U' L)
"""

AF = """
U F' R' F R2 U R2' F R F U F2
U (↑ L U' L' U2) (L2 U F U' F') L' U' L'
U2 F U (R' F R F') (R' U2' R2 U) R2 U R F'
U' (r U R' F') (R U R' U') (R' F R2 U' r')
(F' L' U' L) (F' L F L2' U2 L U) (L' U' L F)
U' (R U R' U R U2' R') U (R U2' R' U2 R' F R F')
"U2 (R f R' f') U (R2' u' R U' R U R' u R)
U2 (R' U' R U) F (R U R' U') (R' U R2 U' R' F')"
(MR U R' U) (R U2' R' U) (R' F R F' r U R')
(R U2 R' U) R' U' F U R U' F' R U2 R'
U (↑ L U L' U) (L2 F' L' F L') U2 (L U' L' U L U' L')
U2 (L' U2 L U2' L F' L' F) (L' U' L U' L' U2 L)
(R' U2' R U') (R' F' U F) U' R2 U2' (R' U' R U') R'
"""

AL = """
"M(L' U' L F L' U' L U L F' L2' U L U) M'
U2 x R2' U2' (R U R' U2) L U' R U M"
U2 (R' U' F' U F·R') (D' R U2 R' D R U2 R)
U2 (r U R' U' M) (U F' R U R' U' R' F R2 U' R')
U2 (F R' F' R U2 R U2' R') U' (R U2' R' U' R U' R')
U2 (F R' F' U R) U2 (R U2' R' U) R' U2' R
"U' (L' u' L U' L' U L' u L2) U (L' f' L f)
U2 F (R U R2' U' R) (U R U' R' F') (U' R' U R)"
U' (l' U' L2 F L') (U' L' U L) (F' L' U l)
U2 (r U R' U') (r' F R2 U' R' U') (R U R' F')
U2 (R' U' R U) (R' U' R U2) (R' F R' F' R2) (U R' U R)
U2 (R U' R' U2) (R' U' F R F') (R' U R2 U' R')
U2 (F R' F' R) U2 (R U' R' U) (R' U' R2 U' R2' U2' R)
U2 (R' F R F') (R' F R2 U R' U') (R' F' R2 U R')
"""

AR = """
U2 x' U (R U' R2' F') (R2 U' R' U') F' U2 x
U2 (R U R' U') F2 (r U r' F2) U (R U' R2' F') R
(↑R' U R U2') (R2' U' F' U F) R U R
U2 (L' U' L U' L' U2 L) U' (L' U2 L U2' L F' L' F)
U (R' F' U' F) R U F (R U R' U') (R' U R U') F'
U2 R2' F R U F' (R U R' U) F U F' U R
U2 (l' U' L F) (L' U' L U) (L F' L2' U l)
U (F R U R') (F R' F' R2 U2' R' U') (R U R' F')
"U' (L' f' L f) U' (L2 u L' U L' U' L u' L')
U' (L U L' U') F' (L' U' L U) (L U' L2' U L F)"
(↑R' U' R U') (R2' F R F' R) U2' (R' U R U' R' U R)
U' (R U R' U R U2' R2') (F R F' U2 F' U2 F)
U' (R U2' R' U2 R' F R F') (R U R' U R U2' R')
"""

update_all("AU", AU)
update_all("AD", AD)
update_all("AB", AB)
update_all("AF", AF)
update_all("AL", AL)
update_all("AR", AR)

BU = """
"U' (S R' F' R S') (R' F R U' M' U2 M)
(M' U2 M U) (R' F' R S R' F R S')"
U' (R U R' S') (R U' R' S)
U (R U' R' S') (R U R' S)
"(↑R2' S2 R F) U' (R' S2 R U) F' R
(r U M U R' U' r U' r') (R U R' U R U2' R')"
U' (R' U' R F D R2 D') (R' F' U R2 U2' R')
U (L U L' F' D' L2' D) (L F U' L2' U2 L)
(M' U' M U) (M' U' M U') M' U2 M
(M' U M U') (M' U M U) M' U2 M
"""

BD = """
U2 (r' U r U') r' F' U' (r U2 R' U' R U' r') F r
U2 (F' U2 F U2') (R U' R' U) (R U R' U) F' U' F
U (↓B r U r' B') (r2 D r' U' r D' r2')
U' (↑F' r' U' r F) (r2' D' r U r' D r2)
U F (R' F R F') U' (F U r U2 r' U' R U R' F2)
(R U' R' U') (F U R U' R' F) (r U2' R' U') r' F R
U R' F' U2 F U' (R U R2' F R F') U2 R
(R U' R2' F R F') (R U' B U2' B' R')
"""

BB = """
U2 (F R' F' R2 U' R') (F' U' F) R U R'
↑R' U' R' (F' U' F U) (R2 U2' R' U' R)
↑ L U L (F U F' U') (L2' U2 L U L')
(R' U R U') R' F U (R' F R F') U' R' F' R2
M' U (r' F R F') U (R U R' U2) r U r'
U2 F (R U' R' U) (R U2' R2' F R F') (R U' R' F')
"U2 F' (L' U L U') (L' U2 L2 F' L' F) (L' U L F)
[oh] U F U' (R U R' U2') R U (R2' F R F') U2 F'"
(L U2 L' U) (L' U2 L U2') (L U F' L' F)
(R' U2' R U') (R U2' R' U2) (R' U' F R F')
U2 F (R U' R' U2') (R U R' U) (R' U2' R2 U) R2 U R F'
U (R U' R2' F R) U (R U' R2' F' R) (F R' F' R)
"U' (L' U L2 F' L') U' (L' U L2 F L') (F' L F L')
[oh] (F U R U2 R' U' R U R' F') U' (R' U' R U' R' U2' R)"
"""

BL = """
U (↑R' U2 R' D' R U2 R' D R) (F' U' F U R)
U (↑ L U2' L D L' U2' L D' L') (F U F' U' L')
U2 R' F' U2 (R' F R U') (R' F' R U') F U' R
U2 (R' U2' R U') R' U' (R' F R F') (R U2' R' U2 R)
U2 (R U2' R' F) U R' U' F' U (R U' R U2' R')
U2 (R' U' R U') F U' (R' U' R U) (R U' R' U') (R U R' F')
U2 (R2 U R' U) (R' U' R U') R' F U R U' F' R'
U' (R U R' U') F' U' (R' U' F' U F R) F U2 (R U' R')
U2 F (R U' R' U) (R U R2' F') (r U R U' r')
F' (L' U L U') (L' U' L2 F) (l' U' L' U l)
U2 (L' U2 L U L' U L) (F' L F L' U2' L' U2 L)
(R U2' R' U' R U' R') (F R' F' R U2 R U2' R')
"""

update_all("BU", BU)
update_all("BD", BD)
update_all("BB", BB)
update_all("BL", BL)


