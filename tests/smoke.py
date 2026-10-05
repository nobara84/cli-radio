#!/usr/bin/env python3
"""Local-only PTY + actual mpv/null-audio integration smoke test. No internet."""
import fcntl
import http.server
import io
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import termios
import threading
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'target' / 'smoke'
WORK.mkdir(parents=True, exist_ok=True)
MPV = shutil.which('mpv')
assert MPV, 'mpv is required'
requests = []
lock = threading.Lock()

class Stream(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        with lock:
            requests.append(self.path)
        finite = '/finite' in self.path
        output = io.BytesIO()
        with wave.open(output, 'wb') as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(8000)
            wav.writeframes(b'\0\0' * (8000 * (2 if finite else 120)))
        payload = output.getvalue()
        self.send_response(200)
        self.send_header('Content-Type', 'audio/wav')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        try:
            if finite:
                self.wfile.write(payload)
                self.wfile.flush()
            else:
                self.wfile.write(payload[:44])
                for start in range(44, len(payload), 4000):
                    self.wfile.write(payload[start:start + 4000])
                    self.wfile.flush()
                    time.sleep(.2)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_CONNECT(self):
        with lock:
            requests.append('CONNECT ' + self.path)
        self.send_response(502)
        self.end_headers()

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Stream)
server.daemon_threads = True
threading.Thread(target=server.serve_forever, daemon=True).start()
port = server.server_port
bindir = WORK / 'bin'
bindir.mkdir(exist_ok=True)
wrapper = bindir / 'mpv'
wrapper.write_text('#!/bin/sh\nexec ' + MPV + ' --ao=null "$@"\n')
wrapper.chmod(0o700)

class Radio:
    def __init__(self, name, proxy=False, finite=False, bypass=False, missing=False):
        self.base = WORK / name
        self.env = os.environ.copy()
        for key in list(self.env):
            if key.lower().endswith('proxy'):
                del self.env[key]
        self.env.update(TERM='xterm-256color', PATH=str(self.base / 'empty-bin') if missing else str(bindir) + ':' + self.env['PATH'])
        for kind in ('CONFIG', 'DATA', 'STATE'):
            dest = self.base / kind.lower()
            dest.mkdir(parents=True, exist_ok=True)
            self.env['XDG_' + kind + '_HOME'] = str(dest)
        config = self.base / 'config' / 'cli-radio'
        config.mkdir(exist_ok=True)
        if proxy or bypass:
            (config / 'config.toml').write_text(f'volume = 85\n[network]\nproxy = "http://127.0.0.1:{port}"\nno_proxy = "' + ('127.0.0.1' if bypass else '') + '"\n')
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 28, 100, 0, 0))
        self.before = termios.tcgetattr(self.slave)
        self.proc = subprocess.Popen([str(ROOT / 'target/debug/cli-radio')], stdin=self.slave, stdout=self.slave, stderr=self.slave, env=self.env)
        self.output = bytearray()
        self.wait(lambda: b'CLI Radio' in self.output, 5)
        url = f'http://127.0.0.1:{port}/' + ('finite' if finite else 'live')
        if proxy:
            url = 'http://radio.invalid/live'
        self.key('aSmoke Station\t' + url + '\r')
        self.wait(lambda: (self.base / 'data/cli-radio/stations.toml').exists(), 5)

    def key(self, key):
        os.write(self.master, key.encode())

    def drain(self):
        if select.select([self.master], [], [], .03)[0]:
            self.output.extend(os.read(self.master, 65536))

    def wait(self, predicate, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.drain()
            if predicate():
                return
            assert self.proc.poll() is None, 'cli-radio exited unexpectedly'
        raise AssertionError('Timeout: ' + self.output[-1000:].decode(errors='replace'))

    def log(self):
        file = self.base / 'state/cli-radio/cli-radio.log'
        return file.read_text() if file.exists() else ''

    def child(self):
        children = Path(f'/proc/{self.proc.pid}/task/{self.proc.pid}/children').read_text().split()
        if not children:
            # Tokio may spawn the child from a worker thread.
            for task in Path(f'/proc/{self.proc.pid}/task').iterdir():
                children += (task / 'children').read_text().split()
        return int(children[0]) if children else None

    def close(self, key='q'):
        child = self.child()
        if key == 'SIGTERM':
            self.proc.send_signal(signal.SIGTERM)
        else:
            self.key(key)
        deadline = time.monotonic() + 6
        while self.proc.poll() is None and time.monotonic() < deadline:
            self.drain()
        assert self.proc.poll() == 0, 'Exit/cleanup failed'
        self.drain()
        assert termios.tcgetattr(self.slave) == self.before, 'Terminal attributes not restored'
        assert b'\x1b[?1049l' in self.output and b'\x1b[?25h' in self.output, 'Screen/cursor cleanup missing'
        assert child is None or not Path(f'/proc/{child}').exists(), 'mpv child leaked'
        os.close(self.master)
        os.close(self.slave)

radios = []
try:
    r = Radio('lifecycle-' + str(os.getpid()))
    radios.append(r)
    r.key('\r')
    r.wait(lambda: ' playing' in r.log())
    child = r.child()
    assert child is not None
    os.kill(child, signal.SIGKILL)
    r.wait(lambda: r.log().count(' playing') >= 2)
    assert r.child() != child
    r.key(' ')
    r.wait(lambda: 'intentional stop' in r.log())
    count = r.log().count('mpv start/restart requested')
    until = time.monotonic() + 2
    while time.monotonic() < until:
        r.drain()
    assert count == r.log().count('mpv start/restart requested'), 'Reconnect after intentional stop'
    r.key('f/e')  # Favorite + search, then return to normal.
    r.key('\x1b')
    r.close()
    radios.remove(r)
    print('PASS: real mpv playback, crash/restart, intentional stop, q cleanup')

    r = Radio('eof-' + str(os.getpid()), finite=True)
    radios.append(r)
    r.key('\r')
    r.wait(lambda: r.log().count(' playing') >= 2, 12)
    assert 'unexpected disconnect' in r.log()
    r.close('\x03')
    radios.remove(r)
    print('PASS: finite HTTP stream EOF reconnect, Ctrl+C cleanup')

    r = Radio('proxy-' + str(os.getpid()), proxy=True)
    radios.append(r)
    r.key('\r')
    r.wait(lambda: ' playing' in r.log())
    assert any(path.startswith('http://radio.invalid') for path in requests), 'HTTP proxy unused'
    # Edit and restart through HTTPS; our local proxy deliberately rejects CONNECT.
    r.key('e\t')
    for _ in 'http://radio.invalid/live':
        r.key('\x7f')
    r.key('https://radio.invalid/live\r\r')
    r.wait(lambda: any(path.startswith('CONNECT radio.invalid:443') for path in requests))
    r.close('SIGTERM')
    radios.remove(r)
    print('PASS: HTTP proxy routes stream, HTTPS uses CONNECT, SIGTERM cleanup')

    start = len(requests)
    r = Radio('bypass-' + str(os.getpid()), bypass=True)
    radios.append(r)
    r.key('\r')
    r.wait(lambda: ' playing' in r.log())
    assert any(path == '/live' for path in requests[start:]), 'NO_PROXY bypass failed'
    assert not any(path.startswith('http://') for path in requests[start:]), 'Bypassed stream went through proxy'
    r.close()
    radios.remove(r)
    print('PASS: explicit NO_PROXY reaches stream directly')

    r = Radio('missing-' + str(os.getpid()), missing=True)
    radios.append(r)
    r.key('\r')
    r.wait(lambda: b'installed' in r.output)
    r.key(' ')
    r.close()
    radios.remove(r)
    print('PASS: missing mpv shows recoverable TUI error')
finally:
    for r in radios:
        child = r.child()
        r.proc.send_signal(signal.SIGTERM)
        try:
            r.proc.wait(timeout=6)
        except subprocess.TimeoutExpired:
            r.proc.kill()
            r.proc.wait()
        if child and Path(f'/proc/{child}').exists():
            os.kill(child, signal.SIGKILL)
    server.shutdown()
