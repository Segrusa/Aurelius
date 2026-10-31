#!/usr/bin/env python3
"""Allow every device that waits to pair with the test daemon, for <seconds>.
usage: allow.py <socket> <user> <password> [seconds]"""
import json, subprocess, sys, time, os
sock, user, pw = sys.argv[1:4]; end = time.time() + float(sys.argv[4] if len(sys.argv) > 4 else 120)
aur = [sys.executable, '-I', os.path.join(os.path.dirname(os.path.abspath(__file__)), 'aur.py'), sock, user, pw]
while time.time() < end:
    st = json.loads(subprocess.run(aur + ['remote.get_status'], capture_output=True, text=True).stdout)
    for p in st['ok']['value'].get('pending', []):
        subprocess.run(aur + ['remote.decide_pairing', json.dumps({'pairing_id': p['pairing_id'], 'approve': True, 'owner': False}), '--write'], capture_output=True)
        print('allowed', p['device_name'], flush=True)
    time.sleep(1)
