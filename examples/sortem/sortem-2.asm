; sortem, phase 2 of 2: cells 64-98 already hold the sorted numbers; print them, then mean, min, max, median
start: CLA k0
       STO i
       STO sum
pr:    CLA opcla      ; build CLA tab+i
       ADD i
       STO ld
ld:    DAT 0
       STO v
       OUT v          ; the sorted numbers
       ADD sum
       STO sum
       CLA i
       ADD one
       STO i
       SUB n
       TAC pr
       CLA k0         ; mean = sum / N, by subtracting
       STO q
dv:    CLA sum
       SUB n
       TAC dvd
       STO sum
       CLA q
       ADD one
       STO q
       JMP dv
dvd:   OUT q          ; mean
       OUT 64         ; min
       OUT 98         ; max
       OUT 81         ; median
       HRS 0
one:   DAT 1
k0:    DAT 0
n:     DAT 35
opcla: CLA 64
i:     DAT 0
sum:   DAT 0
v:     DAT 0
q:     DAT 0
