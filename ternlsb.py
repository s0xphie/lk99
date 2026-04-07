#Usage:
#python tlsb.py <filename> - interpret a TernLSB program (uses embedded save if present)
#python tlsb.py <filename> <input.txt> - interpret with external scripted input (e.g. save.txt)
#python tlsb.py <inputfn> <brainfuckfn> <outputfn> - Encodes brainfuck into an image file
#python tlsb.py <inputfn> <brainfuckfn> <outputfn> <save.txt> - Encodes brainfuck and save data into an image
import sys
from PIL import Image

input_file_lines = None
input_line_index = 0
input_buffer = ""

SAVE_MARKER_LEN = 4  # number of %9==8 bytes that mark embedded save data

def _set_byte(orig, digit):
    """Set a byte's mod-9 value to digit, keeping the upper bits close to original."""
    v = orig // 9 * 9 + digit
    if v >= 256:
        v -= 9
    return v

def _find_terminator(d):
    """Find the program terminator (first byte where %9 == 8) in raw image bytes."""
    for i, b in enumerate(d):
        if b % 9 == 8:
            return i
    return None

def _read_embedded_save(d, term_idx):
    """Read embedded save data from image bytes after the program terminator."""
    pos = term_idx + 1
    if pos + SAVE_MARKER_LEN > len(d):
        return None
    for k in range(SAVE_MARKER_LEN):
        if d[pos + k] % 9 != 8:
            return None
    pos += SAVE_MARKER_LEN
    chars = []
    while pos + 3 <= len(d):
        d0 = d[pos] % 9
        d1 = d[pos + 1] % 9
        d2 = d[pos + 2] % 9
        v = d0 + d1 * 9 + d2 * 81
        if v == 0:
            break
        chars.append(chr(v))
        pos += 3
    if not chars:
        return None
    return ''.join(chars)

def _encode_save(d, term_idx, save_text):
    """Encode save data into image bytes after the program terminator."""
    pos = term_idx + 1
    needed = SAVE_MARKER_LEN + len(save_text) * 3 + 3  # marker + data + NUL terminator
    if pos + needed > len(d):
        print("Image too small to embed save data")
        sys.exit(1)
    # Save marker
    for k in range(SAVE_MARKER_LEN):
        d[pos] = _set_byte(d[pos], 8)
        pos += 1
    # Encode each character as 3 base-9 digits
    for ch in save_text:
        v = ord(ch)
        digits = [v % 9, (v // 9) % 9, (v // 81) % 9]
        for digit in digits:
            d[pos] = _set_byte(d[pos], digit)
            pos += 1
    # NUL terminator
    for k in range(3):
        d[pos] = _set_byte(d[pos], 0)
        pos += 1

def bf(code):
    global input_file_lines, input_line_index, input_buffer
    s1=[]
    s2=[]
    matches={}
    tape=[0]*1000000
    for i,j in enumerate(code):
        if j=='[':
            s1.append(i)
        if j==']':
            m=s1.pop()
            matches[m]=i
            matches[i]=m
    cp=0
    p=0
    while cp<len(code):
        if code[cp]=='+':
            tape[p]=(tape[p]+1)%256
        if code[cp]=='-':
            tape[p]=(tape[p]-1)%256
        if code[cp]==',':
            if input_buffer == "":
                if input_file_lines is not None and input_line_index < len(input_file_lines):
                    input_buffer = input_file_lines[input_line_index] + "\n"
                    input_line_index += 1
                else:
                    try:
                        input_buffer = input() + "\n"
                    except EOFError:
                        input_buffer = "\n"
            tape[p] = ord(input_buffer[0]) % 256
            input_buffer = input_buffer[1:]
        if code[cp]=='.':
            print(chr(tape[p]),end='')
        if code[cp]=='<':
            p-=1
        if code[cp]=='>':
            p+=1
        if code[cp]=='[':
            if not tape[p]:
                cp=matches[cp]
        if code[cp]==']':
            if tape[p]:
                cp=matches[cp]
        cp+=1
def run(fn):
    global input_file_lines, input_line_index
    im=Image.open(fn)
    d=im.tobytes()
    fuck='+-,.<>[]'
    b=''
    for i in d:
        try:
            b+=fuck[i%9]
        except:
            break
    # Check for embedded save if no external input file provided
    if input_file_lines is None:
        term_idx = _find_terminator(d)
        if term_idx is not None:
            save_text = _read_embedded_save(d, term_idx)
            if save_text is not None:
                lines = save_text.split('\n')
                if lines and lines[-1] == '':
                    lines.pop()
                input_file_lines = lines
                input_line_index = 0
    bf(b)
def enc(fn,b,o,save_text=None):
    im=Image.open(fn)
    fuck='+-,.<>[]'
    d=im.tobytes()
    d=list(d)
    w=''
    for i in b:
        if i in fuck:
            w+=i
    for i,j in enumerate(w):
        d[i]=d[i]//9
        d[i]=d[i]*9
        d[i]+=fuck.index(j)
        if d[i]>=256:
            d[i]-=9
    term_idx=len(w)
    d[term_idx]=_set_byte(d[term_idx],8)
    if save_text is not None:
        _encode_save(d, term_idx, save_text)
    db=bytes(d)
    Image.frombytes(im.mode,im.size,db).save(o)
    
if __name__=='__main__':
    a=sys.argv
    if len(a)==2:
        run(a[1])
    elif len(a)==3:
        with open(a[2], 'r', encoding='utf-8') as f:
            input_file_lines = [line.rstrip('\n\r') for line in f]
        run(a[1])
    elif len(a)==4:
        enc(a[1],open(a[2]).read(),a[3])
    elif len(a)==5:
        with open(a[4], 'r', encoding='utf-8') as f:
            save_data = f.read()
        enc(a[1],open(a[2]).read(),a[3],save_text=save_data)
    else:
        print('Must pass 1, 2, 3, or 4 arguments')
