#!/usr/bin/env python3
"""Tiny IPC client for the AUR-MU-03 test daemon (protocol 5): sign in, then one call.
usage: aur.py <socket> <user> <password> <method> [json params] [--write]
       aur.py <socket> - - <method> ...   (no sign-in: onboarding)"""
import json, socket, struct, sys, uuid

def frame(s, obj):
    b = json.dumps(obj).encode(); s.sendall(struct.pack('>I', len(b)) + b)

def read(s):
    def n(k):
        out = b''
        while len(out) < k:
            c = s.recv(k - len(out))
            if not c: raise SystemExit('closed')
            out += c
        return out
    return json.loads(n(struct.unpack('>I', n(4))[0]))

rid = 0
def call(s, method, params=None, write=False):
    global rid; rid += 1
    req = {'protocol': 5, 'request_id': rid, 'method': method}
    if params is not None: req['params'] = params
    if write: req['idempotency_key'] = str(uuid.uuid4())
    frame(s, req)
    while True:
        m = read(s)
        if m.get('request_id') == rid: return m

sock, user, pw, method = sys.argv[1:5]
rest = [a for a in sys.argv[5:] if a != '--write']
params = json.loads(rest[0]) if rest else None
s = socket.socket(socket.AF_UNIX); s.connect(sock)
call(s, 'session.hello', {'client_name': 'mu03-m2-tool', 'client_version': '0', 'accepts': []})
if user != '-':
    r = call(s, 'user.login', {'username': user, 'password': pw, 'device_name': 'test tool'})
    if 'error' in json.dumps(r)[:200] and 'session_token' not in json.dumps(r): print(json.dumps(r)); sys.exit(1)
print(json.dumps(call(s, method, params, '--write' in sys.argv), indent=1))
if user != '-': call(s, 'user.logout')  # no session left behind per call
