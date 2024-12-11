[org 0x7C00]
[bits 16]

jmp short _start
nop

bdb_oem: db "MSWIN4.1"
bdb_bytes_per_sector: dw 512
bdb_sectors_per_cluster: db 1
bdb_reserved_sectors: dw 1
bdb_number_of_fats: db 2
bdb_root_entries: dw 0xe0
bdb_total_sectors: dw 2880
bdb_media_descriptor: db 0xf0
bdb_sectors_per_fat: dw 9
bdb_sectors_per_track: dw 18
bdb_number_of_heads: dw 2
bdb_hidden_sectors: db 0
bdb_total_sectors_big: db 0

ebr_drive_number: db 0x0, 0x0
ebr_signature: db 0x29
ebr_volume_id: db 0x12, 0x34, 0x56, 0x78
ebr_volume_label: db "NO NAME    "
ebr_file_system: db "FAT12   "

; ---- Code ----

_start:
  ; setup segments
  xor ax, ax
  mov ds, ax
  mov es, ax

  mov [ebr_drive_number], dl
  mov ax, 1
  mov cl, 1
  mov bx, 0x7e00
  call _disk_read

  ; setup stack
  mov ss, ax
  mov sp, 0x7c00

  cli ; disable interrupts, so CPU stays in halt
	hlt

_err_n_die:
  mov si, err_msg_read
  jmp $

; convert LBA address to CHS
; params:
; 	- ax: LBA address
; returns:
;	- cx [0-5]: sector number
;	- cx [6-15]: cylinder number
;	- dh: head number
_lba_to_chs:
  ; push values to stack
	push ax
	push dx

	xor dx, dx
	div word [bdb_sectors_per_track]
	inc dx
	mov cx, dx

	xor dx, dx
	div word [bdb_number_of_heads]
	
	mov dh, dl
	mov ch, al
	shl ah, 6
	or cl, ah

	pop ax
	mov dl, al
	pop ax
	ret

; read sectors from disk
; params:
;  - ax: LBA address
;  - cl: number of sectros to read (1-128)
;  - dl: drive number
;  - es:bx mem address where to store the data
_disk_read:
  push ax
  push bx
  push cx
  push dx
  push di

  push cx 
  call _lba_to_chs
  pop ax

  mov ah, 0x02
  mov di, 3

.disk_read_retry:
  pusha
  stc
  int 0x13
  jnc .disk_read_ok

  popa
  call disk_reset

  dec di
  test di, di
  jnz .disk_read_retry

.disk_read_fail:
  jmp _err_n_die

.disk_read_ok:
  popa
  
  pop di
  pop dx
  pop cx
  pop bx
  pop ax
  ret

; reset disk controller
; params:
;  - dl: drive number
disk_reset:
  pusha
  mov ah, 0x0
  stc
  int 0x13
  jc _err_n_die
  popa
  ret

; ---- Data ----

err_msg_read db "ERROR: Failed to read from disk", 0

times 510-($-$$) db 0
dw 0xaa55
