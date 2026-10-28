#!/usr/bin/env python3
"""Turns truecolour ANSI text (`glyphforge export --format ansi`) into an
SVG with one rect and one glyph per cell, so the grid stays exact.

    glyphforge export examples/agent-monitor.glyph --format ansi > am.ans
    scripts/ansi_to_svg.py am.ans > am.svg && rsvg-convert -z 2 am.svg -o am.png
"""
import re, sys, html

data=open(sys.argv[1],encoding='utf-8').read()
W,H=9,19
fg0,bg0='#d0d0d0','#101014'
rows=data.rstrip('\n').split('\n')
out=[]; maxw=0
for y,line in enumerate(rows):
    fg,bg,bold=fg0,None,False;x=0
    for tok in re.findall(r'\x1b\[[0-9;]*m|[^\x1b]',line):
        if tok.startswith('\x1b'):
            p=[int(v) if v else 0 for v in tok[2:-1].split(';')]
            i=0
            while i<len(p):
                v=p[i]
                if v==0: fg,bg,bold=fg0,None,False
                elif v==1: bold=True
                elif v==22: bold=False
                elif v==39: fg=fg0
                elif v==49: bg=None
                elif v in(38,48) and p[i+1]==2:
                    c='#%02x%02x%02x'%tuple(p[i+2:i+5])
                    if v==38: fg=c
                    else: bg=c
                    i+=4
                elif v in(38,48) and p[i+1]==5: i+=2
                i+=1
            continue
        if bg: out.append(f'<rect x="{x*W}" y="{y*H}" width="{W+.5}" height="{H}" fill="{bg}"/>')
        if tok.strip(): out.append(f'<text x="{x*W}" y="{y*H+H*.76}" fill="{fg}"{" font-weight=\"bold\"" if bold else ""}>{html.escape(tok)}</text>')
        x+=1
    maxw=max(maxw,x)
print(f'<svg xmlns="http://www.w3.org/2000/svg" width="{maxw*W}" height="{len(rows)*H}" font-family="JetBrainsMono Nerd Font, DejaVu Sans Mono, monospace" font-size="15"><rect width="100%" height="100%" fill="{bg0}"/>'+'\n'.join(out)+'</svg>')
