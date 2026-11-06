import json,sys
sys.path.insert(0,'/tmp/claude-1000/-home-kaan-markdown-viewer/354efa0d-cde2-4ce7-8746-f07ca6433a9c/scratchpad/verify/py')
from normalize import normalize_html as n
ex=json.load(open('ex.json'));got=json.load(open('got.json'))
miss=[i+1 for i,(e,g) in enumerate(zip(ex,got)) if n(e['html'])!=n(g)]
print('official normalize.py misses',len(miss),miss)
sem=[]
import re
def s(h):
    h=h.replace('style="text-align: left"','align="left"').replace('style="text-align: center"','align="center"').replace('style="text-align: right"','align="right"').replace('<tbody></tbody>','')
    return re.sub(r'"(?![^<]*>)','&quot;',h)
sem=[i+1 for i,(e,g) in enumerate(zip(ex,got)) if s(n(e['html']))!=s(n(g))]
print('python port of semantic misses',len(sem),sem)
