from math import gcd, lcm, isqrt, floor, ceil, trunc
from fractions import Fraction as F
import json
B = 2**128 + 1
out = {}
out['gcd_lcm'] = [[a,b,gcd(a,b),lcm(a,b)] for a,b in [(0,0),(0,-18),(-48,18),(48,-18),(-48,-18),(35,64),(B*21,B*35)]]
out['sqrt'] = [[n,isqrt(n)] for n in [0,1,2,3,4,15,16,17] + [v for k in [2**64+1,2**128+123] for v in [k*k-1,k*k,k*k+1,(k+1)**2-1]]]
out['normalize_round'] = [[a,b,f.numerator,f.denominator,floor(f),ceil(f),trunc(f)] for a,b in [(0,-7),(6,-8),(-6,-8),(7,3),(-7,3),(6,3),(-6,3),(B,3*B)] for f in [F(a,b)]]
def pair(f): return [f.numerator,f.denominator]
out['arithmetic'] = [[pair(a),pair(b),pair(a+b),pair(a-b),pair(a*b),pair(a/b),(a>b)-(a<b)] for a,b in [(F(-2,3),F(5,6)),(F(7,12),F(-14,9)),(F(0),F(-5,7)),(F(B,B-2),F(B-2,B)),(F(2,4),F(1,2))]]
out['failures'] = []
for label, run in [('sqrt(-1)',lambda:isqrt(-1)),('normalize(0,0)',lambda:F(0,0)),('normalize(1,0)',lambda:F(1,0)),('div(1/2,0)',lambda:F(1,2)/F(0)),('div(0,0)',lambda:F(0)/F(0))]:
    try: run()
    except (ValueError, ZeroDivisionError): out['failures'].append(label)
print(json.dumps(out, indent=2))
