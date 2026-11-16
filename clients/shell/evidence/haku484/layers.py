import re,sys,collections,statistics as st
L=open(sys.argv[1]).read().splitlines()
out=[];cur=[]
for l in L:
    if 'phase=layer' in l:
        d=dict(re.findall(r'(\w+)=([\d.]+)',l.split('SHELL_PROFILE',1)[1])); cur.append((d['w']+'x'+d['h'],float(d['ms'])))
    elif 'phase=frame' in l:
        d=dict(re.findall(r'(\w+)=([\d.]+)',l)); out.append((float(d['render_ms']),cur,d)); cur=[]
r=sorted(x[0] for x in out); print('diag frames',len(out),'slow',sum(1 for x in r if x>16),'render p50 %.1f p95 %.1f'%(r[len(r)//2],r[int(.95*len(r))]))
lay=[x for o in out for x in o[1]]; print('layer renders',len(lay),'sum %.0f ms'%sum(x[1] for x in lay))
c=collections.Counter(a for a,b in lay); print(c.most_common(8))
k=[n for n,o in enumerate(out) if o[0]>30]
if k:
  for o in out[k[len(k)//2]-2:k[len(k)//2]+12]: print('%.1f'%o[0],'geo',o[2]['geometry_ms'],'txt',o[2]['text_draw_ms'],[f'{a}:{b:.1f}' for a,b in o[1]])
