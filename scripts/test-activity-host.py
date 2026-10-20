"""Opt-in Windows tray/toast smoke test, dummy job only; no backup files accessed."""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

parser = argparse.ArgumentParser(__doc__)
parser.add_argument('--show-notifications', action='store_true', help='Send two real Windows test notifications')
parser.add_argument('--error', action='store_true', help='Show error icon and verify automatic exit after 30 seconds')
parser.add_argument('--cancelled', action='store_true', help='Verify a normal cancellation notification without an error icon')
parser.add_argument('--measure', action='store_true', help='Sample host working set and CPU over two idle seconds')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
package = Path(os.environ.get('NAS_BACKUP_TEST_PACKAGE', root / 'portable/NAS Backup Manager'))
host = package / 'nas-backup-notify.exe'
application = package / 'NAS Backup Manager.exe'
request = dict(application=str(application), job='Проверка уведомлений: Документы', language='ru', notifications=args.show_notifications, owner=os.getpid())
process = subprocess.Popen([str(host)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8', creationflags=subprocess.CREATE_NO_WINDOW)
messages = queue.Queue()
def read_messages():
    for line in process.stdout:
        messages.put(json.loads(line))
reader = threading.Thread(target=read_messages, daemon=True)
reader.start()
def send(value):
    process.stdin.write(json.dumps(value, ensure_ascii=False) + '\n')
    process.stdin.flush()
def sample():
    command = "$activityProcess=Get-Process -Id " + str(process.pid) + "; @{workingSet=$activityProcess.WorkingSet64; peakWorkingSet=$activityProcess.PeakWorkingSet64; cpuMs=$activityProcess.TotalProcessorTime.TotalMilliseconds} | ConvertTo-Json -Compress"
    response = subprocess.check_output(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command], creationflags=subprocess.CREATE_NO_WINDOW)
    return json.loads(response)
try:
    send(request)
    started = messages.get(timeout=10)
    assert started['stage'] == 'started' and started['error'] is None, started
    assert started['job'] == request['job'], 'UTF-8 did not round-trip'
    if args.measure:
        first = sample()
        began = time.monotonic()
        time.sleep(2)
        last = sample()
        elapsed = time.monotonic() - began
        print(json.dumps(dict(sampleSeconds=round(elapsed, 3), cpuMs=last['cpuMs']-first['cpuMs'], workingSet=last['workingSet'], peakWorkingSet=last['peakWorkingSet'])), flush=True)
    send(dict(action='finish', result='failed' if args.error else 'cancelled' if args.cancelled else 'succeeded'))
    finished = messages.get(timeout=10)
    assert finished['stage'] == ('finished_error' if args.error else 'finished'), finished
    assert finished['error'] is None and finished['job'] == request['job'], finished
    assert finished['notifications'] == (2 if args.show_notifications else 0), finished
    if args.error:
        assert process.poll() is None, 'Error icon disappeared immediately'
    process.stdin.close()
    assert process.wait(timeout=35) == 0
    reader.join(timeout=1)
    print(json.dumps(dict(started=started, finished=finished), ensure_ascii=True))
finally:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
