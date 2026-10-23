; sortem, phase 1 of 2: read 35 numbers into cells 64-98 and sort them in place, then HRS 2
; (load the phase 2 tape over this one; the sorted numbers stay in memory)
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
sorted: HRS 2
one:   DAT 1
k0:    DAT 0
n:     DAT 35
opcla: CLA 64
i:     DAT 0
opinp: INP 64
opsto: STO 64
j:     DAT 0
a:     DAT 0
b:     DAT 0
pass:  DAT 0
