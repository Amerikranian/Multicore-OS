section .text
    global _start

_start:
    mov rax, 1
    int 0x80

    mov rbx, 5000000000
    mov rax, 35
    int 0x80

    mov rax, 60
    int 0x80
