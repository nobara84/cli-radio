#!/usr/bin/env python3
"""Local-only PTY + actual mpv/null-audio integration smoke test. No internet."""
import fcntl
import http.server
import io
import os
from pathlib import Path
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import termios
import threading
import time
import tomllib
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
        self.proc = subprocess.Popen([str(ROOT / 'target/release/cli-radio')], stdin=self.slave, stdout=self.slave, stderr=self.slave, env=self.env)
        self.output = bytearray()
        try:
            self.wait(lambda: b'CLI Radio' in self.output, 5)
            # Brand-new settings and data must still produce a usable empty German UI.
            if not (config / 'config.toml').exists():
                self.wait(lambda: b'NochkeineSender' in self.plain(), 5)
            url = f'http://127.0.0.1:{port}/' + ('finite' if finite else 'live')
            if proxy:
                url = 'http://radio.invalid/live'
            self.key('aSmoke Station\t' + url + '\r')
            self.wait(lambda: (self.base / 'data/cli-radio/stations.toml').exists() and 'Smoke Station' in (self.base / 'data/cli-radio/stations.toml').read_text(), 5)
        except BaseException:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(timeout=6)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
            os.close(self.master)
            os.close(self.slave)
            raise

    def key(self, key):
        os.write(self.master, key.encode())

    def plain(self):
        text = re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]', b'', self.output)
        return re.sub(rb'\s+', b'', text)

    def screen(self):
        # Ratatui sends differential frames; unchanged letters are omitted.
        # Reconstruct cursor-positioned cells rather than searching raw bytes.
        cells = [[' ' for _ in range(100)] for _ in range(28)]
        x = y = 0
        output = self.output.decode(errors='replace')
        cursor = 0
        for match in re.finditer(r'\x1b\[([0-?]*)([ -/]*)([@-~])', output):
            for char in output[cursor:match.start()]:
                if char == '\r':
                    x = 0
                elif char == '\n':
                    y += 1
                elif char.isprintable():
                    if x >= 100:
                        x = 0
                        y += 1
                    if 0 <= y < 28 and 0 <= x < 100:
                        cells[y][x] = char
                    x += 1
            params, _, final = match.groups()
            values = [int(v) if v else 1 for v in params.split(';')] if not params.startswith('?') else []
            if final in ('H', 'f') and values:
                y = values[0] - 1
                x = (values[1] if len(values) > 1 else 1) - 1
            elif final == 'J' and params == '2':
                cells = [[' ' for _ in range(100)] for _ in range(28)]
            elif final == 'K' and 0 <= y < 28:
                for col in range(max(0, x), 100):
                    cells[y][col] = ' '
            cursor = match.end()
        return '\n'.join(''.join(row) for row in cells)

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

    def settings(self):
        path = self.base / 'config/cli-radio/config.toml'
        return tomllib.loads(path.read_text()) if path.exists() else {}

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
    assert b'Sender' in r.output and b'|' in r.output
    r.key('l')
    r.wait(lambda: r.settings().get('language') == 'en')
    r.key('l')
    r.wait(lambda: r.settings().get('language') == 'de')
    r.key('r')
    r.wait(lambda: r.settings().get('random_mode') is True)
    r.key('\r')
    r.wait(lambda: ' playing' in r.log())
    r.wait(lambda: b'11:' in r.output)
    r.key('r')
    r.wait(lambda: r.settings().get('random_mode') is False)
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
    # Add a second synthetic station and intentionally switch to it.
    count = r.log().count(' playing')
    r.key('aAlternate\thttp://127.0.0.1:' + str(port) + '/live?second\r')
    r.wait(lambda: 'Alternate' in (r.base / 'data/cli-radio/stations.toml').read_text())
    r.key('\x1b[B\r')
    r.wait(lambda: r.log().count(' playing') > count)
    r.key('f')
    r.wait(lambda: any(s['favorite'] for s in tomllib.loads((r.base / 'data/cli-radio/stations.toml').read_text())['stations']))
    r.key('l')
    r.wait(lambda: r.settings().get('language') == 'en')
    r.key('r')
    r.wait(lambda: r.settings().get('random_mode') is True)
    name = r.base.name
    r.key('/e')  # Search, then return to normal.
    r.key('\x1b')
    r.close()
    radios.remove(r)
    print('PASS: real mpv playback, crash/restart, stop, manual switch, favorite, language/random keys, q cleanup')
    r = Radio(name)
    radios.append(r)
    assert r.settings()['language'] == 'en' and r.settings()['random_mode'] is True
    assert b'NowPlaying' in r.plain()
    r.close()
    radios.remove(r)
    print('PASS: persisted English/random choice survives restart without autoplay')

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
    r.wait(lambda: 'mpv ist nicht installiert' in r.screen() or 'mpv is not installed' in r.screen())
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
