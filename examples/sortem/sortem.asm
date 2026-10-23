; sortem: read 2 numbers, sort them, print them sorted, then mean, min, max, median
; everything in one tape: 94 cells of code and variables leave room for only 2 numbers (cells 97-98)
start: CLA k0
       STO i
rd:    CLA opinp      ; build INP tab+i
       ADD i
       STO rdi
rdi:   DAT 0          ; INP tab+i
       CLA i
       ADD one
       STO i
       SUB n
       TAC rd
       CLA n          ; N-1 passes of bubble sort
       SUB one
       STO pass
ps:    CLA k0
       STO j
cmp:   CLA opcla      ; build CLA tab+j and CLA tab+j+1
       ADD j
       STO la
       ADD one
       STO lb
la:    DAT 0
       STO a
lb:    DAT 0
       STO b
       SUB a          ; b - a < 0: out of order
       TAC swap
next:  CLA j
       ADD one
       STO j
       ADD one
       SUB n
       TAC cmp
       CLA pass
       SUB one
       STO pass
       SUB one
       TAC sorted
       JMP ps
swap:  CLA opsto      ; build STO tab+j and STO tab+j+1
       ADD j
       STO sa
       ADD one
       STO sb
       CLA b
sa:    DAT 0
       CLA a
sb:    DAT 0
       JMP next
sorted:     CLA k0
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
       OUT 97         ; min
       OUT 98         ; max
       OUT 98         ; median
       HRS 0
one:   DAT 1
k0:    DAT 0
n:     DAT 2
opcla: CLA 97
i:     DAT 0
opinp: INP 97
opsto: STO 97
j:     DAT 0
a:     DAT 0
b:     DAT 0
pass:  DAT 0
sum:   DAT 0
v:     DAT 0
q:     DAT 0
tab:   DAT 0
       DAT 0
