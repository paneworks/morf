#!/usr/bin/python3
"""Run under dbus-run-session; uses disposable GCR prompts, never a keyring."""
import json
import os
from pathlib import Path
import queue
import socket
import stat
import subprocess
import threading
import time
import gi

gi.require_version('Gcr', os.environ.get('TEST_GCR_VERSION', '4'))
from gi.repository import Gcr, Gio, GLib

root = Path(__file__).resolve().parents[2]
bridge = root / 'target/dist/morf-keyring'
child = subprocess.Popen([str(bridge)], stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
events = queue.Queue()
transcript = []

def read():
    for line in child.stdout:
        transcript.append(line)
        events.put(json.loads(line))
threading.Thread(target=read, daemon=True).start()
context = GLib.MainContext.default()

def spin(predicate, timeout=5):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        while context.pending():
            context.iteration(False)
        if predicate():
            return
        if child.poll() is not None:
            raise AssertionError('Bridge exited: ' + child.stderr.read())
        time.sleep(.005)
    raise AssertionError('Timed out')


def event(kind):
    result = []
    def poll():
        while not events.empty():
            item = events.get_nowait()
            if item['event'] == kind:
                result.append(item)
                return True
        return False
    spin(poll)
    return result[0]


def reply(item, action='continue', password=None, choice=False):
    payload=json.dumps(dict(id=item['id'], action=action, password=password, choice=choice))+'\n'
    if os.environ.get('TEST_KEYRING_SOCKET') == '1':
        with socket.socket(socket.AF_UNIX) as peer:
            peer.settimeout(3); peer.connect(item['endpoint'])
            # Also exercise fragmented input from another output's UI.
            peer.sendall(payload[:8].encode()); peer.sendall(payload[8:].encode())
            assert peer.recv(64) in (b'ok\n', b'closed\n')
        return
    child.stdin.write(payload)
    child.stdin.flush()


def open_prompt():
    found = []
    Gcr.SystemPrompt.open_async(3, None, lambda _source, result, *_args: found.append(Gcr.SystemPrompt.open_finish(result)), None)
    spin(lambda: bool(found))
    assert found[0] is not None
    return found[0]


def password(prompt):
    result = []
    def done(obj, response, *_):
        result.append(obj.password_finish(response))
    prompt.password_async(None, done, None)
    return result, event('prompt')

try:
    status = event('status')
    if len(status['names']) < 2:
        spin(lambda: not events.empty())
        status = event('status')
    assert len(status['names']) == 2
    p = open_prompt()
    p.set_title('Test keyring'); p.set_message('Unlock disposable test prompt')
    p.set_description('No real account or keyring is used.')
    result, item = password(p)
    assert item['kind'] == 'password' and item['properties']['title'] == 'Test keyring'
    endpoint=Path(item['endpoint'])
    assert stat.S_IMODE(endpoint.stat().st_mode)==0o600
    assert stat.S_IMODE(endpoint.parent.stat().st_mode)==0o700
    with socket.socket(socket.AF_UNIX) as bad:
        bad.settimeout(3); bad.connect(str(endpoint)); bad.sendall(b'{malformed}\n')
        assert bad.recv(64)==b'closed\n'
    assert not result and child.poll() is None, 'Malformed peer cancelled the prompt or killed the bridge'
    secret = 'Disposable-π-🔑-password'
    reply(item, password=secret)
    spin(lambda: bool(result))
    assert result[0] == secret, 'GCR encrypted exchange did not return the exact password'
    assert all(secret not in line for line in transcript), 'Secret leaked to bridge output'
    print('PASS encrypted password exchange and no secret in UI events')

    p.set_warning('Test rejection; enter another password')
    result, item = password(p)
    assert item['properties']['warning'].startswith('Test rejection')
    reply(item, action='cancel')
    spin(lambda: bool(result)); assert result == [None]
    print('PASS retry warning and cancellation')

    p.set_password_new(True); p.set_choice_label('Remember for this session')
    result, item = password(p)
    assert item['properties']['password-new'] and item['properties']['choice-label']
    reply(item, password=secret, choice=True)
    spin(lambda: bool(result)); assert result[0] == secret and p.get_choice_chosen() and p.get_password_strength()>0
    p.close()
    print('PASS new password and choice round-trip')

    p = open_prompt(); result = []
    p.confirm_async(None, lambda obj, response, *_: result.append(obj.confirm_finish(response)), None)
    item = event('prompt'); assert item['kind'] == 'confirm'
    reply(item); spin(lambda: bool(result)); assert result[0] == Gcr.PromptReply.CONTINUE
    p.close()
    print('PASS confirmation prompt')

    p = open_prompt(); result, stale = password(p)
    reply(stale, action='cancel'); spin(lambda: bool(result)); p.close()
    p = open_prompt(); result, current = password(p)
    reply(stale, password='must-not-be-used')
    time.sleep(.05)
    while context.pending(): context.iteration(False)
    assert not result, 'Stale reply answered a different prompt'
    reply(current, action='cancel'); spin(lambda: bool(result)); p.close()
    print('PASS stale reply isolation')
finally:
    child.stdin.close()
    try: child.wait(timeout=5)
    except subprocess.TimeoutExpired: child.kill(); child.wait(); raise
    stderr = child.stderr.read()
    assert child.returncode == 0, stderr
    assert not stderr.strip(), stderr
    assert not endpoint.exists() and not endpoint.parent.exists()
    print('PASS parent disconnect shuts down bridge cleanly')
