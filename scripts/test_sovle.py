cs = 'GBBWWBGRYYOYGOYOOORWORGGWYYBWWWRORGRRROYBRGOWGBBGYYBBW'
from rubik.cube import Cube
from rubik.solve import Solver
c = Cube(cs)
s = Solver(c)
s.solve()